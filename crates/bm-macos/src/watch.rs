//! Watching folders for changes with FSEvents, so that mirrored documentation follows
//! the files while the app runs. Events arrive on a queue of their own, a little
//! delayed and merged, so a save that writes several files comes as one call.

use std::{
    ffi::{CStr, OsStr, c_char, c_void},
    os::unix::ffi::OsStrExt,
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    ptr::{null, null_mut},
    time::Duration,
};

use objc2::rc::Retained;
use objc2_foundation::{NSArray, NSString};

/// Something changed at `path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub path: PathBuf,
    pub is_dir: bool,
    /// Events were merged or lost, or the watched folder itself moved: anything at
    /// or below `path` may have changed.
    pub rescan: bool,
}

type Handler = Box<dyn Fn(Vec<Change>) + Send + Sync>;
type Stream = *mut c_void;
type Queue = *mut c_void;
type Callback = extern "C" fn(Stream, *mut c_void, usize, *mut c_void, *const u32, *const u64);

#[repr(C)]
struct Context {
    version: isize,
    info: *mut c_void,
    retain: Option<extern "C" fn(*const c_void) -> *const c_void>,
    release: Option<extern "C" fn(*const c_void)>,
    copy_description: Option<extern "C" fn(*const c_void) -> *const c_void>,
}

const SINCE_NOW: u64 = u64::MAX;
const CREATE_NO_DEFER: u32 = 0x02;
const CREATE_WATCH_ROOT: u32 = 0x04;
const CREATE_FILE_EVENTS: u32 = 0x10;
const EVENT_MUST_SCAN_SUBDIRS: u32 = 0x01;
const EVENT_USER_DROPPED: u32 = 0x02;
const EVENT_KERNEL_DROPPED: u32 = 0x04;
const EVENT_ROOT_CHANGED: u32 = 0x20;
const EVENT_ITEM_IS_DIR: u32 = 0x0002_0000;

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn FSEventStreamCreate(
        allocator: *const c_void,
        callback: Callback,
        context: *const Context,
        paths: *const c_void,
        since_when: u64,
        latency: f64,
        flags: u32,
    ) -> Stream;
    fn FSEventStreamSetDispatchQueue(stream: Stream, queue: Queue);
    fn FSEventStreamStart(stream: Stream) -> u8;
    fn FSEventStreamStop(stream: Stream);
    fn FSEventStreamInvalidate(stream: Stream);
    fn FSEventStreamRelease(stream: Stream);
}

// libdispatch, part of libSystem.
unsafe extern "C" {
    fn dispatch_queue_create(label: *const c_char, attr: *const c_void) -> Queue;
    fn dispatch_sync_f(queue: Queue, context: *mut c_void, work: extern "C" fn(*mut c_void));
    fn dispatch_release(object: *mut c_void);
}

extern "C" fn on_events(
    _stream: Stream,
    info: *mut c_void,
    count: usize,
    paths: *mut c_void,
    flags: *const u32,
    _ids: *const u64,
) {
    // SAFETY: `info` is the handler given to the stream, alive until `release`;
    // FSEvents passes `count` C strings and as many flags.
    let (handler, paths, flags) = unsafe {
        (
            &*(info as *const Handler),
            std::slice::from_raw_parts(paths as *const *const c_char, count),
            std::slice::from_raw_parts(flags, count),
        )
    };
    let changes = paths
        .iter()
        .zip(flags)
        .map(|(path, flag)| Change {
            // SAFETY: each path is a NUL-terminated string FSEvents owns for this call.
            path: PathBuf::from(OsStr::from_bytes(unsafe { CStr::from_ptr(*path) }.to_bytes())),
            is_dir: flag & EVENT_ITEM_IS_DIR != 0,
            rescan: flag
                & (EVENT_MUST_SCAN_SUBDIRS
                    | EVENT_USER_DROPPED
                    | EVENT_KERNEL_DROPPED
                    | EVENT_ROOT_CHANGED)
                != 0,
        })
        .collect();
    // Unwinding into FSEvents would abort the app.
    let _ = catch_unwind(AssertUnwindSafe(|| handler(changes)));
}

extern "C" fn release_handler(info: *const c_void) {
    // SAFETY: made by `Box::into_raw` in `Watcher::new`, released once by FSEvents.
    drop(unsafe { Box::from_raw(info as *mut Handler) });
}

extern "C" fn nothing(_: *mut c_void) {}

/// Watches until dropped. Dropping waits for a call in progress, so the handler must
/// not drop its own watcher.
pub struct Watcher {
    stream: Stream,
    queue: Queue,
}

// SAFETY: the stream and queue are only touched in `new` and `drop`; FSEvents and
// libdispatch are thread-safe for these calls.
unsafe impl Send for Watcher {}

impl Watcher {
    /// Calls `on_change` with what changed in or below `paths`, at most once per
    /// `latency`. `None` if no path can be watched.
    pub fn new(
        paths: &[PathBuf],
        latency: Duration,
        on_change: impl Fn(Vec<Change>) + Send + Sync + 'static,
    ) -> Option<Self> {
        let strings: Vec<Retained<NSString>> =
            paths.iter().filter_map(|p| p.to_str()).map(NSString::from_str).collect();
        if strings.is_empty() {
            return None;
        }
        let array = NSArray::from_retained_slice(&strings);
        let handler: Handler = Box::new(on_change);
        let info = Box::into_raw(Box::new(handler));
        let context = Context {
            version: 0,
            info: info.cast(),
            retain: None,
            release: Some(release_handler),
            copy_description: None,
        };
        // SAFETY: an NSArray of NSStrings is a CFArray of CFStrings (toll-free
        // bridged); FSEvents copies the context and the paths.
        unsafe {
            let stream = FSEventStreamCreate(
                null(),
                on_events,
                &context,
                Retained::as_ptr(&array).cast(),
                SINCE_NOW,
                latency.as_secs_f64(),
                CREATE_NO_DEFER | CREATE_WATCH_ROOT | CREATE_FILE_EVENTS,
            );
            if stream.is_null() {
                drop(Box::from_raw(info));
                return None;
            }
            let queue = dispatch_queue_create(c"app.brainmatter.watch".as_ptr(), null());
            FSEventStreamSetDispatchQueue(stream, queue);
            if FSEventStreamStart(stream) == 0 {
                FSEventStreamInvalidate(stream);
                FSEventStreamRelease(stream);
                dispatch_release(queue);
                return None;
            }
            Some(Self { stream, queue })
        }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        // SAFETY: the stream was started in `new` and is torn down once, here.
        unsafe {
            FSEventStreamStop(self.stream);
            FSEventStreamInvalidate(self.stream);
            // A call that is already running ends before the handler is freed.
            dispatch_sync_f(self.queue, null_mut(), nothing);
            FSEventStreamRelease(self.stream);
            dispatch_release(self.queue);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::mpsc,
        time::{Duration, Instant},
    };

    use super::*;

    #[test]
    fn reports_a_file_written_in_a_watched_folder_and_stops_when_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let watcher =
            Watcher::new(std::slice::from_ref(&root), Duration::from_millis(50), move |changes| {
                let _ = tx.lock().unwrap().send(changes);
            })
            .unwrap();
        fs::write(root.join("Seite.md"), "# Neu").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut seen = false;
        while !seen && Instant::now() < deadline {
            if let Ok(changes) = rx.recv_timeout(Duration::from_millis(200)) {
                seen = changes.iter().any(|c| c.path == root.join("Seite.md") && !c.is_dir);
            }
        }
        assert!(seen, "the write is reported");
        drop(watcher);
        while rx.try_recv().is_ok() {}
        fs::write(root.join("Später.md"), "# Später").unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(500)).is_err(), "nothing after drop");
    }
}
