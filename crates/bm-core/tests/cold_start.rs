//! SQLCipher sets itself up once per process, when SQLite is first used. Every
//! thread that keys a connection at that moment must still succeed.

use std::{
    fs,
    process::{Command, Stdio},
    sync::Barrier,
    thread,
};

use bm_core::{DbKey, backups};

const THREADS: usize = 8;
const PROCESSES: usize = 1000;

#[test]
#[ignore = "needs a fresh process, see connections_can_be_keyed_right_after_start"]
fn first_connections_are_keyed_in_parallel() {
    let dir = tempfile::tempdir().unwrap();
    let start = Barrier::new(THREADS);
    thread::scope(|s| {
        for i in 0..THREADS {
            // Checking an empty backup does the least work before keying, so
            // the race shows most often.
            let path = dir.path().join(format!("{i}.db"));
            fs::File::create(&path).unwrap();
            let start = &start;
            s.spawn(move || {
                let key = DbKey::from_bytes([i as u8; 32]);
                start.wait();
                backups::verify(&path, &key).unwrap();
            });
        }
    });
}

#[test]
fn connections_can_be_keyed_right_after_start() {
    let exe = std::env::current_exe().unwrap();
    // The race shows far more often while every CPU is busy.
    let batch = 2 * thread::available_parallelism().unwrap().get();
    for _ in 0..PROCESSES.div_ceil(batch) {
        let children: Vec<_> = (0..batch)
            .map(|_| {
                Command::new(&exe)
                    .args(["--exact", "first_connections_are_keyed_in_parallel", "--ignored"])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap()
            })
            .collect();
        for child in children {
            let out = child.wait_with_output().unwrap();
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(
                out.status.success() && stdout.contains("1 passed"),
                "{stdout}{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}
