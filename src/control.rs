use eldenring::cs::{
    BonfireWarpParam, CSEventFlagMan, CSTaskGroupIndex, CSTaskImp, ChrIns, GameMan,
    SoloParamRepository, WorldChrMan, WorldChrManDbgFlags,
};
use eldenring::fd4::FD4TaskData;
use eldenring::rotation::Quaternion;
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
            let mut s = SHARED.lock().unwrap();
            while let Some((id, cmd)) = s.pending.pop_front() {
                let out = run(&cmd, frame, &run_dir);
                s.done.push((id, out));
            }
            crate::drive::step();
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
        ["chars"] => {
            let Ok(w) = (unsafe { WorldChrMan::instance() }) else {
                return "error no WorldChrMan\n".into();
            };
            let mut out = String::from("ok rank\thp\tmax_hp\tchara_init_param\tworld\n");
            for (i, c) in by_hp(w).iter().take(10).enumerate() {
                let c = unsafe { c.as_ref() };
                let (d, p) = (&c.modules.data, &c.modules.physics.position);
                let _ = writeln!(
                    out,
                    "{}\t{}\t{}\t{}\t{:.2} {:.2} {:.2}",
                    i + 1,
                    d.hp,
                    d.max_hp,
                    d.chara_init_param_id,
                    p.0,
                    p.1,
                    p.2
                );
            }
            out
        }
        ["targethp", pct, rest @ ..] if rest.len() <= 1 => {
            let Ok(pct) = pct.parse::<u32>() else {
                return format!("error not a percentage: {pct}\n");
            };
            let Ok(w) = (unsafe { WorldChrMan::instance() }) else {
                return "error no WorldChrMan\n".into();
            };
            // A rank, as `chars` lists them, picks a character over the target.
            let t = match rest {
                [rank] => match rank.parse::<usize>() {
                    Ok(r) if r > 0 => by_hp(w).get(r - 1).copied(),
                    _ => return format!("error not a rank: {rank}\n"),
                },
                _ => target(w),
            };
            let Some(mut t) = t else {
                return "error no target\n".into();
            };
            let data = &mut unsafe { t.as_mut() }.modules.data;
            data.hp = (data.max_hp as i64 * pct.min(100) as i64 / 100) as i32;
            format!("ok target hp={}/{}\n", data.hp, data.max_hp)
        }
        ["loadslot", slot] => {
            let Ok(slot) = slot.parse::<i32>() else {
                return format!("error not a slot: {slot}\n");
            };
            match unsafe { GameMan::instance_mut() } {
                Ok(g) => {
                    g.requested_save_slot_load_index = slot;
                    format!("ok requested_save_slot_load_index={slot}\n")
                }
                Err(e) => format!("error no GameMan: {e:?}\n"),
            }
        }
        ["tp", rest @ ..] if rest.len() == 3 || rest.len() == 7 => {
            let Ok(v) = rest
                .iter()
                .map(|a| a.parse::<f32>())
                .collect::<Result<Vec<_>, _>>()
            else {
                return "error usage: tp X Y Z [Q0 Q1 Q2 Q3] (world position and facing, as status shows them)\n".into();
            };
            let Some(p) = (unsafe { WorldChrMan::instance_mut() })
                .ok()
                .and_then(|w| w.main_player.as_mut())
            else {
                return "error no player\n".into();
            };
            let physics = &mut p.chr_ins.modules.physics;
            physics.position.0 = v[0];
            physics.position.1 = v[1];
            physics.position.2 = v[2];
            // Facing matters for what comes next: an action button such as a
            // fog gate's only takes the player within an angle of it.
            if let [_, _, _, a, b, c, d] = v[..] {
                physics.orientation = Quaternion(a, b, c, d);
                physics.interpolated_orientation = Quaternion(a, b, c, d);
            }
            // Asks the character controller to take the new position too, or
            // it may keep its own and move the player back.
            physics.chr_proxy_pos_update_requested = true;
            format!("ok tp {}\n", rest.join(" "))
        }
        ["lockhp", pct] => {
            let Ok(pct) = pct.parse::<u32>() else {
                return format!("error not a percentage: {pct}\n");
            };
            crate::drive::lock(Some(pct.min(100)));
            "ok lockhp facing the target to lock on; see lockon in status\n".into()
        }
        ["lockon"] => {
            crate::drive::lock(None);
            "ok lockon facing the target to lock on; see lockon in status\n".into()
        }
        ["face"] => {
            let Ok(w) = (unsafe { WorldChrMan::instance_mut() }) else {
                return "error no WorldChrMan\n".into();
            };
            let Some(t) = target(w) else {
                return "error no target\n".into();
            };
            match crate::drive::face(w, t) {
                Ok(yaw) => format!("ok face yaw={yaw:.1}\n"),
                Err(e) => format!("error {e}\n"),
            }
        }
        ["move", angle, rest @ ..] if rest.len() <= 1 => {
            let Ok(angle) = angle.parse::<f32>() else {
                return format!("error not an angle: {angle}\n");
            };
            let frames = match rest {
                [] => 1,
                [n] => match n.parse::<u32>() {
                    Ok(n) if n > 0 => n,
                    _ => return format!("error not a frame count: {n}\n"),
                },
                _ => unreachable!(),
            };
            crate::drive::push(angle, frames);
            format!("ok move {angle} frames={frames}\n")
        }
        ["walkto", rest @ ..] if (2..=4).contains(&rest.len()) => {
            let Ok(v) = rest
                .iter()
                .map(|a| a.parse::<f32>())
                .collect::<Result<Vec<_>, _>>()
            else {
                return "error usage: walkto X Z [RADIUS [SECONDS]]\n".into();
            };
            let radius = v.get(2).copied().unwrap_or(1.0);
            let secs = v.get(3).copied().unwrap_or(30.0);
            crate::drive::walk(v[0], v[1], radius, (secs * 60.0) as u32);
            format!("ok walkto {} {} radius={radius}; see walk in status\n", v[0], v[1])
        }
        ["stop"] => {
            crate::drive::stop();
            crate::input::release();
            "ok stopped walks, lock-ons and held keys\n".into()
        }
        ["give", id, rest @ ..] if rest.len() <= 1 => {
            let id = id
                .strip_prefix("0x")
                .map_or_else(|| id.parse(), |h| u32::from_str_radix(h, 16));
            let Ok(id) = id else {
                return "error not an item id (decimal or 0x..)\n".into();
            };
            let qty = match rest {
                [] => 1,
                [n] => match n.parse() {
                    Ok(n) if n > 0 => n,
                    _ => return format!("error not a quantity: {n}\n"),
                },
                _ => unreachable!(),
            };
            match crate::item::give(id, qty) {
                Ok(info) => format!("ok give 0x{id:08x} x{qty} {info}\n"),
                Err(e) => format!("error {e}\n"),
            }
        }
        ["asm"] => match crate::equip::dump() {
            Ok(out) => out,
            Err(e) => format!("error {e}\n"),
        },
        ["equip", id, rest @ ..] if rest.len() <= 1 => {
            let id = id
                .strip_prefix("0x")
                .map_or_else(|| id.parse(), |h| u32::from_str_radix(h, 16));
            let Ok(id) = id else {
                return "error not an item id (decimal or 0x..)\n".into();
            };
            let slot = match rest {
                [] => 1,
                [n] => match n.parse() {
                    Ok(n) => n,
                    _ => return format!("error not a slot: {n}\n"),
                },
                _ => unreachable!(),
            };
            match crate::equip::equip(id, slot) {
                Ok(info) => format!("ok equip {info}\n"),
                Err(e) => format!("error {e}\n"),
            }
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
        [kind @ ("keypress" | "mousepress"), code, rest @ ..] => {
            let code = code
                .strip_prefix("0x")
                .map_or_else(|| code.parse(), |h| u8::from_str_radix(h, 16));
            let Ok(code) = code else {
                return "error not a key code (decimal or 0x..)\n".into();
            };
            let frames = match rest {
                [] => 1,
                [n] => match n.parse() {
                    Ok(n) if n > 0 => n,
                    _ => return format!("error not a frame count: {n}\n"),
                },
                _ => return format!("error usage: {kind} CODE [FRAMES]\n"),
            };
            let target = match *kind {
                "keypress" => crate::input::Target::Dik(code),
                _ => crate::input::Target::Mouse(code),
            };
            crate::input::hold(target, frames);
            format!("ok {kind} {code} frames={frames}\n")
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
            let target = match (crate::input::key(name), crate::input::stick(name)) {
                (Some(k), _) => crate::input::Target::Key(k),
                (_, Some(k)) => crate::input::Target::Stick(k, 1.0),
                _ => return format!("error unknown key: {name}\n"),
            };
            let frames = match rest {
                [] => 1,
                [n] => match n.parse() {
                    Ok(n) if n > 0 => n,
                    _ => return format!("error not a frame count: {n}\n"),
                },
                _ => return "error usage: press KEY [FRAMES]\n".into(),
            };
            crate::input::hold(target, frames);
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

/// Loaded characters other than the player, most max HP first. A fight can
/// hold several big ones, such as Rykard hidden under the arena while his
/// serpent fights, so they are picked by rank.
fn by_hp(w: &WorldChrMan) -> Vec<NonNull<ChrIns>> {
    let Some(p) = w.main_player.as_ref() else {
        return Vec::new();
    };
    let me: *const ChrIns = &p.chr_ins;
    let mut v: Vec<_> = w
        .chr_inses_by_distance
        .iter()
        .map(|e| e.chr_ins)
        .filter(|c| !std::ptr::eq(c.as_ptr(), me))
        .collect();
    v.sort_by_key(|c| std::cmp::Reverse(unsafe { c.as_ref() }.modules.data.max_hp));
    v
}

/// The enemy the player is locked on to, else the nearest of the three
/// loaded characters with the most max HP. Nearest, not toughest: a fight can
/// keep its next phase's character loaded out of reach, as Rykard waits under
/// his arena with more HP than the serpent the player fights.
pub fn target(w: &WorldChrMan) -> Option<NonNull<ChrIns>> {
    let p = w.main_player.as_ref()?;
    if let Some(t) = w.chr_ins_by_handle(&p.locked_on_enemy) {
        return Some(NonNull::from(t));
    }
    let me = &p.chr_ins.modules.physics.position;
    let dist = |c: &NonNull<ChrIns>| {
        let t = &unsafe { c.as_ref() }.modules.physics.position;
        let (dx, dy, dz) = (t.0 - me.0, t.1 - me.1, t.2 - me.2);
        dx * dx + dy * dy + dz * dz
    };
    by_hp(w)
        .into_iter()
        .take(3)
        .min_by(|a, b| dist(a).total_cmp(&dist(b)))
}

fn status(frame: u64) -> String {
    let mut s = format!("key\tvalue\nframe\t{frame}\n");
    // What loading a save changes here, watched from the title screen while
    // the player loads by hand, is what a scripted load has to reproduce.
    if let Ok(g) = unsafe { GameMan::instance() } {
        let _ = writeln!(
            s,
            "save_slot\t{}\nrequested_slot\t{}\nsave_state\t{}",
            g.save_slot, g.requested_save_slot_load_index, g.save_state
        );
    }
    crate::drive::status(&mut s);
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
    let world = &p.chr_ins.modules.physics.position;
    let q = &p.chr_ins.modules.physics.orientation;
    let _ = write!(
        s,
        "stage\tin_game\nhp\t{}\nmax_hp\t{}\nblock\t{}\npos\t{:.2} {:.2} {:.2}\nworld\t{:.2} {:.2} {:.2}\nfacing\t{:.4} {:.4} {:.4} {:.4}\n",
        data.hp,
        data.max_hp,
        p.current_block_id,
        pos.x,
        pos.y,
        pos.z,
        world.0,
        world.1,
        world.2,
        q.0,
        q.1,
        q.2,
        q.3
    );
    if let Ok(w) = unsafe { WorldChrMan::instance() } {
        // Characters the game has loaded around the player; a boss's arena
        // shows more once the boss is in.
        let _ = writeln!(s, "chars\t{}", w.chr_inses_by_distance.len());
        let locked = w.chr_ins_by_handle(&p.locked_on_enemy).is_some();
        let _ = writeln!(s, "locked\t{}", if locked { "on" } else { "off" });
        if let Some(t) = target(w) {
            let d = &unsafe { t.as_ref() }.modules.data;
            let _ = writeln!(s, "target_hp\t{}\ntarget_max_hp\t{}", d.hp, d.max_hp);
            let tp = &unsafe { t.as_ref() }.modules.physics.position;
            let me = &p.chr_ins.modules.physics.position;
            let (dx, dy, dz) = (tp.0 - me.0, tp.1 - me.1, tp.2 - me.2);
            let _ = writeln!(
                s,
                "target_dist\t{:.1}",
                (dx * dx + dy * dy + dz * dz).sqrt()
            );
            let _ = writeln!(s, "target_world\t{:.2} {:.2} {:.2}", tp.0, tp.1, tp.2);
        }
    }
    s
}
