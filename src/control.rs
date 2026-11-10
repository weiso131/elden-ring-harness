use eldenring::cs::{
    BonfireWarpParam, CSEventFlagMan, CSTaskGroupIndex, CSTaskImp, ChrIns, SoloParamRepository,
    WorldChrMan, WorldChrManDbgFlags,
};
use eldenring::fd4::FD4TaskData;
use fromsoftware_shared::{FromStatic, SharedTaskImpExt};
use std::collections::{BTreeSet, VecDeque};
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::Mutex;
use std::time::Duration;

/// Commands waiting for the main thread, and what it reports back.
struct Shared {
    pending: VecDeque<(String, String)>,
    done: Vec<(String, String)>,
    status: String,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared {
    pending: VecDeque::new(),
    done: Vec::new(),
    status: String::new(),
});

/// Serves commands from `dir/cmd/<id>.cmd`, answering in `<id>.out`, and keeps
/// `dir/status.tsv` current. Files are polled from this thread; commands run on
/// the main thread inside the game loop, where touching game state is safe.
pub fn serve(dir: &Path, interval: Duration) -> io::Result<()> {
    let cmd_dir = dir.join("cmd");
    fs::create_dir_all(&cmd_dir)?;
    let task = CSTaskImp::wait_for_instance(Duration::MAX)
        .map_err(|e| io::Error::other(format!("CSTaskImp not found: {e:?}")))?;
    let mut frame = 0u64;
    let run_dir = dir.to_path_buf();
    let _input = crate::input::install(task);
    // FrameBegin always has the game's own tasks, so it adds no group of its own.
    let _handle = task.run_recurring(
        move |_: &FD4TaskData| {
            frame += 1;
            crate::input::hold_focus();
            let mut s = SHARED.lock().unwrap();
            while let Some((id, cmd)) = s.pending.pop_front() {
                let out = run(&cmd, frame, &run_dir);
                s.done.push((id, out));
            }
            s.status = status(frame);
        },
        CSTaskGroupIndex::FrameBegin,
    );

    let mut seen = BTreeSet::new();
    loop {
        std::thread::sleep(interval);
        let mut new = Vec::new();
        for e in fs::read_dir(&cmd_dir)?.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if let Some(id) = name.strip_suffix(".cmd")
                && seen.insert(id.to_string())
            {
                new.push((
                    id.to_string(),
                    fs::read_to_string(e.path())?.trim().to_string(),
                ));
            }
        }
        // Ids sort in the order the CLI made them.
        new.sort();
        let (done, status) = {
            let mut s = SHARED.lock().unwrap();
            s.pending.extend(new);
            (std::mem::take(&mut s.done), s.status.clone())
        };
        for (id, out) in done {
            write_atomic(&cmd_dir.join(format!("{id}.out")), &out)?;
        }
        if !status.is_empty() {
            write_atomic(&dir.join("status.tsv"), &status)?;
        }
    }
}

/// Readers never see a half-written file.
fn write_atomic(path: &Path, text: &str) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text)?;
    fs::rename(tmp, path)
}

fn run(cmd: &str, frame: u64, dir: &Path) -> String {
    let args: Vec<&str> = cmd.split_whitespace().collect();
    match args.as_slice() {
        ["ping"] => format!("ok frame={frame}\n"),
        ["warp", id] => match id.parse() {
            Ok(id) => match crate::warp::to_grace(id) {
                Ok(entity) => format!("ok warping grace={id} entity={entity}\n"),
                Err(e) => format!("error {e}\n"),
            },
            Err(_) => format!("error not a grace id: {id}\n"),
        },
        ["flag", id, rest @ ..] => {
            let Ok(id) = id.parse::<u32>() else {
                return format!("error not a flag id: {id}\n");
            };
            let Ok(man) = (unsafe { CSEventFlagMan::instance_mut() }) else {
                return "error event flags not ready\n".into();
            };
            let flags = &mut man.virtual_memory_flag;
            match rest {
                [] => {}
                ["on"] => flags.set_flag(id, true),
                ["off"] => flags.set_flag(id, false),
                _ => return "error usage: flag ID [on|off]\n".into(),
            }
            let state = if flags.get_flag(id) { "on" } else { "off" };
            format!("ok flag {id}={state}\n")
        }
        ["targethp", pct] => {
            let Ok(pct) = pct.parse::<u32>() else {
                return format!("error not a percentage: {pct}\n");
            };
            let Some(mut t) = unsafe { WorldChrMan::instance() }.ok().and_then(target) else {
                return "error no target\n".into();
            };
            let data = &mut unsafe { t.as_mut() }.modules.data;
            data.hp = (data.max_hp as i64 * pct.min(100) as i64 / 100) as i32;
            format!("ok target hp={}/{}\n", data.hp, data.max_hp)
        }
        ["probe", name] => match crate::input::key(name) {
            Some(key) => {
                crate::input::probe(key, 120);
                format!("ok probing {name} for 120 frames; see probe in status\n")
            }
            None => format!("error unknown key: {name}\n"),
        },
        ["scan"] => {
            crate::input::scan(120);
            "ok scanning raw input for 120 frames; see scan in status\n".into()
        }
        ["rawpress", idx, rest @ ..] => {
            let Ok(idx) = idx.parse() else {
                return format!("error not an index: {idx}\n");
            };
            let frames = match rest {
                [] => 1,
                [n] => match n.parse() {
                    Ok(n) if n > 0 => n,
                    _ => return format!("error not a frame count: {n}\n"),
                },
                _ => return "error usage: rawpress INDEX [FRAMES]\n".into(),
            };
            crate::input::hold(crate::input::Target::Raw(idx), frames);
            format!("ok rawpress {idx} frames={frames}\n")
        }
        ["press", name, rest @ ..] => {
            let Some(key) = crate::input::key(name) else {
                return format!("error unknown key: {name}\n");
            };
            let frames = match rest {
                [] => 1,
                [n] => match n.parse() {
                    Ok(n) if n > 0 => n,
                    _ => return format!("error not a frame count: {n}\n"),
                },
                _ => return "error usage: press KEY [FRAMES]\n".into(),
            };
            crate::input::hold(crate::input::Target::Key(key), frames);
            format!("ok press {name} frames={frames}\n")
        }
        ["graces"] => match dump_graces(&dir.join("graces.tsv")) {
            Ok(n) => format!("ok graces rows={n} in graces.tsv\n"),
            Err(e) => format!("error {e}\n"),
        },
        ["flagdump", name] => match dump_flags(&dir.join(format!("flags-{name}.txt"))) {
            Ok(n) => format!("ok flagdump {name} on={n}\n"),
            Err(e) => format!("error {e}\n"),
        },
        [
            name @ ("nodead" | "exterminate" | "nohit"),
            on @ ("on" | "off"),
        ] => {
            match unsafe { WorldChrManDbgFlags::instance_mut() } {
                Ok(f) => {
                    let on = *on == "on";
                    match *name {
                        // The game keeps the player's HP at 1 instead of letting it reach 0.
                        "nodead" => f.player_no_dead = on,
                        // Every hit by the player deals 9999999 damage.
                        "exterminate" => f.player_exterminate = on,
                        // Enemy attacks do not hit the player.
                        _ => f.all_no_hit = on,
                    }
                    format!("ok {name}={}\n", if on { "on" } else { "off" })
                }
                Err(e) => format!("error debug flags not found: {e:?}\n"),
            }
        }
        _ => format!("error unknown command: {cmd}\n"),
    }
}

/// Lists every grace: its row id for `warp`, the flag that unlocks it, and the
/// grace's cleared flag (`clearedEventFlagId`), with their current state. On a
/// boss grace the cleared flag is expected to be the boss's defeated flag.
fn dump_graces(path: &Path) -> Result<usize, String> {
    let params = unsafe { SoloParamRepository::instance() }.map_err(|e| format!("{e:?}"))?;
    let flags = &unsafe { CSEventFlagMan::instance() }
        .map_err(|e| format!("{e:?}"))?
        .virtual_memory_flag;
    let on = |f: u32| if f != 0 && flags.get_flag(f) { 1 } else { 0 };
    let mut out = String::from(
        "row\tentity\tarea\tgrid_x\tgrid_z\tunlock_flag\tunlocked\tcleared_flag\tcleared\n",
    );
    let mut n = 0;
    for (id, row) in params.rows::<BonfireWarpParam>() {
        let (unlock, cleared) = (row.eventflag_id(), row.cleared_event_flag_id());
        let _ = writeln!(
            out,
            "{id}\t{}\t{}\t{}\t{}\t{unlock}\t{}\t{cleared}\t{}",
            row.bonfire_entity_id(),
            row.area_no(),
            row.grid_x_no(),
            row.grid_z_no(),
            on(unlock),
            on(cleared)
        );
        n += 1;
    }
    fs::write(path, out).map_err(|e| e.to_string())?;
    Ok(n)
}

/// Writes every flag that is on, one id per line, so two dumps can be diffed to
/// find the flags an event sets. Reads each bit through get_flag on the main
/// thread, so it stalls the game: meant for setting up experiments, not for a
/// measured run.
fn dump_flags(path: &Path) -> Result<usize, String> {
    let man = unsafe { CSEventFlagMan::instance() }.map_err(|e| format!("{e:?}"))?;
    let flags = &man.virtual_memory_flag;
    let divisor = flags.event_flag_divisor;
    let mut out = String::new();
    let mut n = 0;
    for entry in flags.flag_block_descriptors.iter() {
        let base = entry.first * divisor;
        for id in base..base + divisor {
            if flags.get_flag(id) {
                let _ = writeln!(out, "{id}");
                n += 1;
            }
        }
    }
    fs::write(path, out).map_err(|e| e.to_string())?;
    Ok(n)
}

/// The enemy the player is locked on to, else the loaded character with the
/// most max HP, which in a boss arena is normally the boss.
fn target(w: &WorldChrMan) -> Option<NonNull<ChrIns>> {
    let p = w.main_player.as_ref()?;
    if let Some(t) = w.chr_ins_by_handle(&p.locked_on_enemy) {
        return Some(NonNull::from(t));
    }
    let me: *const ChrIns = &p.chr_ins;
    w.chr_inses_by_distance
        .iter()
        .map(|e| e.chr_ins)
        .filter(|c| !std::ptr::eq(c.as_ptr(), me))
        .max_by_key(|c| unsafe { c.as_ref() }.modules.data.max_hp)
}

fn status(frame: u64) -> String {
    let mut s = format!("key\tvalue\nframe\t{frame}\n");
    if let Some(on) = crate::input::focused() {
        let _ = writeln!(s, "focused\t{}", if on { "on" } else { "off" });
    }
    if let Some(c) = crate::input::checks(eldenring::cs::UserInputKey::Attack) {
        let _ = writeln!(s, "attack_checks\t{c:?}");
    }
    if let Some(on) = crate::input::polling() {
        let _ = writeln!(s, "pad_polling\t{}", if on { "on" } else { "off" });
    }
    if let Some(sc) = crate::input::scan_status() {
        let _ = writeln!(s, "scan\t{sc}");
    }
    if let Some(p) = crate::input::probe_status() {
        let _ = writeln!(s, "probe\t{p}");
    }
    if let Ok(f) = unsafe { WorldChrManDbgFlags::instance() } {
        let state = |b: bool| if b { "on" } else { "off" };
        let _ = writeln!(s, "nodead\t{}", state(f.player_no_dead));
        let _ = writeln!(s, "exterminate\t{}", state(f.player_exterminate));
    }
    let player = unsafe { WorldChrMan::instance() }
        .ok()
        .and_then(|w| w.main_player.as_ref());
    let Some(p) = player else {
        s.push_str("stage\tno_player\n");
        return s;
    };
    let data = &p.chr_ins.modules.data;
    let pos = &p.block_position;
    let _ = write!(
        s,
        "stage\tin_game\nhp\t{}\nmax_hp\t{}\nblock\t{}\npos\t{:.2} {:.2} {:.2}\n",
        data.hp, data.max_hp, p.current_block_id, pos.x, pos.y, pos.z
    );
    if let Some(t) = unsafe { WorldChrMan::instance() }.ok().and_then(target) {
        let d = &unsafe { t.as_ref() }.modules.data;
        let _ = writeln!(s, "target_hp\t{}\ntarget_max_hp\t{}", d.hp, d.max_hp);
    }
    s
}
