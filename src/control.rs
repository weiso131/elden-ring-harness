use eldenring::cs::{CSTaskGroupIndex, CSTaskImp, WorldChrMan, WorldChrManDbgFlags};
use eldenring::fd4::FD4TaskData;
use fromsoftware_shared::{FromStatic, SharedTaskImpExt};
use std::collections::{BTreeSet, VecDeque};
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::Path;
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
    // FrameBegin always has the game's own tasks, so it adds no group of its own.
    let _handle = task.run_recurring(
        move |_: &FD4TaskData| {
            frame += 1;
            let mut s = SHARED.lock().unwrap();
            while let Some((id, cmd)) = s.pending.pop_front() {
                let out = run(&cmd, frame);
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

fn run(cmd: &str, frame: u64) -> String {
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
        ["nodead", on @ ("on" | "off")] => match unsafe { WorldChrManDbgFlags::instance_mut() } {
            Ok(f) => {
                // The game keeps the player's HP at 1 instead of letting it reach 0.
                f.player_no_dead = *on == "on";
                format!("ok nodead={on}\n")
            }
            Err(e) => format!("error debug flags not found: {e:?}\n"),
        },
        _ => format!("error unknown command: {cmd}\n"),
    }
}

fn status(frame: u64) -> String {
    let mut s = format!("key\tvalue\nframe\t{frame}\n");
    if let Ok(f) = unsafe { WorldChrManDbgFlags::instance() } {
        let _ = writeln!(s, "nodead\t{}", if f.player_no_dead { "on" } else { "off" });
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
    s
}
