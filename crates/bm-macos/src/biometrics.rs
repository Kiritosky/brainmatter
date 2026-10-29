//! Touch ID via LocalAuthentication.
//!
//! Without an Apple Developer account the biometric check gates *reading* the
//! Touch ID key from the Keychain (see PLAN.md §5); it is not bound to the
//! Secure Enclave.

use std::sync::mpsc;

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_foundation::{NSError, NSString};
use objc2_local_authentication::{LABiometryType, LAContext, LAPolicy};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Biometry {
    TouchId,
    FaceId,
    OpticId,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("biometrics unavailable: {0}")]
    Unavailable(String),
    #[error("authentication failed or was cancelled: {0}")]
    Denied(String),
}

fn describe(e: &NSError) -> String {
    format!("{} (code {})", e.localizedDescription(), e.code())
}

/// Which biometry is enrolled and usable right now. Never shows a prompt.
pub fn available() -> Result<Biometry, AuthError> {
    let ctx = unsafe { LAContext::new() };
    unsafe { ctx.canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthenticationWithBiometrics) }
        .map_err(|e| AuthError::Unavailable(describe(&e)))?;
    match unsafe { ctx.biometryType() } {
        LABiometryType::TouchID => Ok(Biometry::TouchId),
        LABiometryType::FaceID => Ok(Biometry::FaceId),
        LABiometryType::OpticID => Ok(Biometry::OpticId),
        _ => Err(AuthError::Unavailable("no biometry enrolled".into())),
    }
}

/// Shows the system biometric prompt and blocks until the user responds.
/// Call from a background thread, never from the main/UI thread.
pub fn authenticate(reason: &str) -> Result<(), AuthError> {
    let ctx = unsafe { LAContext::new() };
    let (tx, rx) = mpsc::sync_channel(1);
    let reply = RcBlock::new(move |ok: Bool, err: *mut NSError| {
        let result = if ok.as_bool() {
            Ok(())
        } else {
            let msg = unsafe { err.as_ref() }.map(describe).unwrap_or_else(|| "unknown".into());
            Err(AuthError::Denied(msg))
        };
        let _ = tx.send(result);
    });
    unsafe {
        ctx.evaluatePolicy_localizedReason_reply(
            LAPolicy::DeviceOwnerAuthenticationWithBiometrics,
            &NSString::from_str(reason),
            &reply,
        );
    }
    rx.recv().unwrap_or_else(|_| Err(AuthError::Denied("reply block dropped".into())))
}
