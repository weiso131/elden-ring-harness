//! Talks to the elden-ring-harness mod through its run directory.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const USAGE: &str = "usage: erh [-d RUN_DIR] status | erh [-d RUN_DIR] COMMAND [ARGS...]
RUN_DIR defaults to $ERH_DIR, else the newest directory in ./out";
const TIMEOUT: Duration = Duration::from_secs(5);

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let dir = if args.first().is_some_and(|a| a == "-d") && args.len() > 1 {
        let d = PathBuf::from(args.remove(1));
        args.remove(0);
        Some(d)
    } else {
        std::env::var_os("ERH_DIR").map(PathBuf::from)
    };
    let Some(dir) = dir.or_else(newest_run) else {
        eprintln!("no run directory\n{USAGE}");
        return ExitCode::FAILURE;
    };
    match args.first().map(String::as_str) {
        None | Some("-h" | "--help") => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
        Some("status") => status(&dir),
        Some(_) => send(&dir, &args.join(" ")),
    }
}

fn newest_run() -> Option<PathBuf> {
    fs::read_dir("out")
        .ok()?
        .flatten()
        .filter(|e| e.path().join("status.tsv").exists())
        .max_by_key(|e| e.file_name())
        .map(|e| e.path())
}

fn status(dir: &Path) -> ExitCode {
    let path = dir.join("status.tsv");
    let Ok(text) = fs::read_to_string(&path) else {
        eprintln!(
            "{}: no status yet; is the game running with the mod?",
            path.display()
        );
        return ExitCode::FAILURE;
    };
    // The mod rewrites it every 50 ms, so an old file means the game is gone.
    let age = fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|m| m.elapsed().ok());
    if let Some(age) = age.filter(|a| *a > Duration::from_secs(1)) {
        println!("stale\t{:.1}s old", age.as_secs_f64());
    }
    print!(
        "{}",
        text.lines()
            .skip(1)
            .map(|l| format!("{l}\n"))
            .collect::<String>()
    );
    ExitCode::SUCCESS
}

fn send(dir: &Path, cmd: &str) -> ExitCode {
    let cmd_dir = dir.join("cmd");
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp = cmd_dir.join(format!("{id}.tmp"));
    if let Err(e) = fs::write(&tmp, format!("{cmd}\n"))
        .and_then(|_| fs::rename(&tmp, cmd_dir.join(format!("{id}.cmd"))))
    {
        eprintln!("{}: {e}", cmd_dir.display());
        return ExitCode::FAILURE;
    }
    let out = cmd_dir.join(format!("{id}.out"));
    let start = Instant::now();
    while start.elapsed() < TIMEOUT {
        if let Ok(text) = fs::read_to_string(&out) {
            print!("{text}");
            return if text.starts_with("ok") {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    eprintln!(
        "no answer in {}s; is the game running with the mod?",
        TIMEOUT.as_secs()
    );
    ExitCode::FAILURE
}
