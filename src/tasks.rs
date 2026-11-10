use crate::clock;
use eldenring::cs::{CSTaskGroupIndex, CSTaskImp};
use eldenring::fd4::FD4TaskData;
use fromsoftware_shared::SharedTaskImpExt;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

/// (t_ns, linux tid, task group) for every time a marker task ran.
static MARKS: Mutex<Vec<(u64, u32, u32)>> = Mutex::new(Vec::new());

/// Registers a marker task in each group in `wanted` (names; `all` for every
/// group) and writes when each ran. Groups run in a fixed order each frame, so
/// the gap between consecutive markers is roughly the time of the groups in
/// between; the order of tasks inside a group is not fixed, so a marker may sit
/// anywhere in its group.
///
/// A marker itself costs about 2 us, but groups the game seems to leave empty
/// (the debug ones) took about 150 us per frame once marked: marking all 169
/// halved the fps.
pub fn record(out: &Path, wanted: &[String], interval: Duration) -> io::Result<()> {
    fs::create_dir_all(out)?;
    let task = CSTaskImp::wait_for_instance(Duration::MAX)
        .map_err(|e| io::Error::other(format!("CSTaskImp not found: {e:?}")))?;
    let freq = clock::qpc_freq();
    fs::write(
        out.join("meta.tsv"),
        format!(
            "key\tvalue\npid\t{}\nqpc_freq\t{freq}\n",
            clock::linux_pid()
        ),
    )?;

    let mut groups = BufWriter::new(File::create(out.join("groups.tsv"))?);
    writeln!(groups, "group\tname")?;
    let mut handles = Vec::new();
    for i in 0..=CSTaskGroupIndex::FrameEnd as u32 {
        // SAFETY: the enum is repr(u32) with no gaps up to FrameEnd.
        let group: CSTaskGroupIndex = unsafe { std::mem::transmute(i) };
        let name = format!("{group:?}");
        writeln!(groups, "{i}\t{name}")?;
        if !wanted.iter().any(|w| w == "all" || *w == name) {
            continue;
        }
        handles.push(task.run_recurring(
            move |_: &FD4TaskData| {
                let mark = (clock::qpc_ns(freq), clock::linux_tid(), i);
                MARKS.lock().unwrap().push(mark);
            },
            group,
        ));
    }
    groups.flush()?;

    let mut w = BufWriter::new(File::create(out.join("tasks.tsv"))?);
    writeln!(w, "t_ns\ttid\tgroup")?;
    let mut batch = Vec::new();
    // Dropping a handle cancels its task, so they live as long as this loop.
    let _handles = handles;
    loop {
        std::thread::sleep(interval);
        std::mem::swap(&mut batch, &mut *MARKS.lock().unwrap());
        for (t, tid, g) in batch.drain(..) {
            writeln!(w, "{t}\t{tid}\t{g}")?;
        }
        w.flush()?;
    }
}
