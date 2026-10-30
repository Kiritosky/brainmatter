//! System events that should lock the vault: sleep and screen lock.

use std::{ptr::NonNull, sync::Arc};

use block2::RcBlock;
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_app_kit::{NSWorkspace, NSWorkspaceWillSleepNotification};
use objc2_foundation::{
    NSDistributedNotificationCenter, NSNotification, NSNotificationCenter, NSObjectProtocol,
    NSOperationQueue, NSString,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockTrigger {
    SystemSleep,
    ScreenLocked,
}

type Token = Retained<ProtocolObject<dyn NSObjectProtocol>>;

/// Keeps the observers registered; dropping it unregisters them.
pub struct PowerObserver {
    tokens: Vec<(Retained<NSNotificationCenter>, Token)>,
}

impl Drop for PowerObserver {
    fn drop(&mut self) {
        for (center, token) in &self.tokens {
            unsafe { center.removeObserver(token.as_ref()) };
        }
    }
}

/// Calls `callback` on the main thread when the Mac goes to sleep or the
/// screen gets locked.
pub fn observe(callback: impl Fn(LockTrigger) + Send + Sync + 'static) -> PowerObserver {
    let callback = Arc::new(callback);
    let queue = NSOperationQueue::mainQueue();
    let mut tokens = Vec::new();

    let mut add =
        |center: Retained<NSNotificationCenter>, name: &NSString, trigger: LockTrigger| {
            let cb = callback.clone();
            let block = RcBlock::new(move |_: NonNull<NSNotification>| cb(trigger));
            let token = unsafe {
                center.addObserverForName_object_queue_usingBlock(
                    Some(name),
                    None,
                    Some(&queue),
                    &block,
                )
            };
            tokens.push((center, token));
        };

    let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
    add(workspace, unsafe { NSWorkspaceWillSleepNotification }, LockTrigger::SystemSleep);

    let distributed = Retained::into_super(NSDistributedNotificationCenter::defaultCenter());
    add(distributed, &NSString::from_str("com.apple.screenIsLocked"), LockTrigger::ScreenLocked);

    PowerObserver { tokens }
}
