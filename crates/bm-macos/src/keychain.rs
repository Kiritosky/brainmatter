//! Generic-password items in the login Keychain, accessed without ever
//! showing a Keychain dialog.
//!
//! Without an Apple Team ID, macOS binds an item's access to the creating
//! binary's cdhash, so a rebuilt or updated app cannot read items written by an
//! older build. Instead of prompting, reads then fail with
//! [`KeychainError::NotAccessible`]; the caller falls back to the password and
//! re-enrolls under a fresh account name (PLAN.md §5).

use security_framework::{
    os::macos::keychain::SecKeychain,
    passwords::{delete_generic_password, get_generic_password, set_generic_password},
};

const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
const ERR_SEC_AUTH_FAILED: i32 = -25293;
const ERR_SEC_INTERACTION_NOT_ALLOWED: i32 = -25308;

#[derive(Debug, thiserror::Error)]
pub enum KeychainError {
    /// The item exists but this binary may not read it without user approval.
    #[error("keychain item belongs to another build of the app")]
    NotAccessible,
    #[error("keychain error {code}: {message}")]
    Other { code: i32, message: String },
}

impl From<security_framework::base::Error> for KeychainError {
    fn from(e: security_framework::base::Error) -> Self {
        match e.code() {
            // With UI disabled, an ACL mismatch surfaces as either code.
            ERR_SEC_AUTH_FAILED | ERR_SEC_INTERACTION_NOT_ALLOWED => Self::NotAccessible,
            code => Self::Other { code, message: e.message().unwrap_or_default() },
        }
    }
}

/// Runs `f` with Keychain UI disabled for the process.
fn silently<T>(
    f: impl FnOnce() -> Result<T, security_framework::base::Error>,
) -> Result<T, KeychainError> {
    let _guard = SecKeychain::disable_user_interaction()?;
    Ok(f()?)
}

pub fn store_secret(service: &str, account: &str, secret: &[u8]) -> Result<(), KeychainError> {
    silently(|| set_generic_password(service, account, secret))
}

pub fn load_secret(service: &str, account: &str) -> Result<Option<Vec<u8>>, KeychainError> {
    match silently(|| get_generic_password(service, account)) {
        Ok(secret) => Ok(Some(secret)),
        Err(KeychainError::Other { code: ERR_SEC_ITEM_NOT_FOUND, .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn delete_secret(service: &str, account: &str) -> Result<(), KeychainError> {
    match silently(|| delete_generic_password(service, account)) {
        Err(KeychainError::Other { code: ERR_SEC_ITEM_NOT_FOUND, .. }) => Ok(()),
        other => other,
    }
}
