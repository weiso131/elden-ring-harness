use std::cell::Cell;
use std::fs;
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

pub fn qpc_freq() -> u64 {
    let mut f = 0;
    let _ = unsafe { QueryPerformanceFrequency(&mut f) };
    f as u64
}

/// QueryPerformanceCounter in ns. Under Wine it is derived from a Linux
/// monotonic clock, but which one has to be checked against glm's timestamps.
pub fn qpc_ns(freq: u64) -> u64 {
    let mut c = 0;
    let _ = unsafe { QueryPerformanceCounter(&mut c) };
    (c as u128 * 1_000_000_000 / freq as u128) as u64
}

thread_local! {
    static TID: Cell<Option<u32>> = const { Cell::new(None) };
}

/// The Linux tid, so rows line up with glm's threads.tsv; Windows thread ids
/// under Wine are unrelated. Wine opens files on the calling thread, so
/// /proc/thread-self is this thread. 0 if it cannot be read.
pub fn linux_tid() -> u32 {
    TID.with(|t| {
        *t.get()
            .get_or_insert_with(|| first_field("Z:/proc/thread-self/stat"))
    })
}

pub fn linux_pid() -> u32 {
    first_field("Z:/proc/self/stat")
}

fn first_field(path: &str) -> u32 {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.split_whitespace().next()?.parse().ok())
        .unwrap_or(0)
}
