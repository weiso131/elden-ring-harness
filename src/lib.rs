mod clock;
mod tasks;

use std::path::PathBuf;
use std::time::Duration;

/// Output directory, as a Linux path; the mod stays inert when it is unset, so
/// a profile that loads the DLL by mistake changes nothing.
const OUT_ENV: &str = "ERH_OUT";
/// Comma-separated task group names to mark, or `all`.
const GROUPS_ENV: &str = "ERH_GROUPS";
const DEFAULT_GROUPS: &str = "FrameBegin";

#[unsafe(no_mangle)]
/// # Safety
///
/// Called by the Windows loader; do not call it yourself.
pub unsafe extern "C" fn DllMain(_hmodule: usize, reason: u32) -> bool {
    const DLL_PROCESS_ATTACH: u32 = 1;
    if reason != DLL_PROCESS_ATTACH {
        return true;
    }
    let Some(out) = std::env::var_os(OUT_ENV).map(|p| wine_path(PathBuf::from(p))) else {
        return true;
    };
    let groups = std::env::var(GROUPS_ENV).unwrap_or_else(|_| DEFAULT_GROUPS.into());
    let groups: Vec<String> = groups.split(',').map(|g| g.trim().to_string()).collect();
    // The task runner is created by the main thread after DllMain returns.
    std::thread::spawn(move || {
        if let Err(e) = tasks::record(&out, &groups, Duration::from_millis(100)) {
            let _ = std::fs::write(out.join("error.txt"), format!("{e}\n"));
        }
    });
    true
}

/// Wine maps the Linux root to drive Z:.
fn wine_path(p: PathBuf) -> PathBuf {
    match p.to_str() {
        Some(s) if s.starts_with('/') => PathBuf::from(format!("Z:{s}")),
        _ => p,
    }
}
