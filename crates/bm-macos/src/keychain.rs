//! Generic-password items in the login Keychain.
//!
//! Item access is bound to the app's code signature, so builds must be signed
//! with a stable identity (self-signed certificate) or macOS will ask for
//! permission again after every rebuild.

use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

#[derive(Debug, thiserror::Error)]
#[error("keychain error {code}: {message}")]
pub struct KeychainError {
    pub code: i32,
    pub message: String,
}

impl From<security_framework::base::Error> for KeychainError {
    fn from(e: security_framework::base::Error) -> Self {
        Self { code: e.code(), message: e.message().unwrap_or_default() }
    }
}

pub fn store_secret(service: &str, account: &str, secret: &[u8]) -> Result<(), KeychainError> {
    Ok(set_generic_password(service, account, secret)?)
}

pub fn load_secret(service: &str, account: &str) -> Result<Option<Vec<u8>>, KeychainError> {
    match get_generic_password(service, account) {
        Ok(secret) => Ok(Some(secret)),
        Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn delete_secret(service: &str, account: &str) -> Result<(), KeychainError> {
    match delete_generic_password(service, account) {
        Err(e) if e.code() != ERR_SEC_ITEM_NOT_FOUND => Err(e.into()),
        _ => Ok(()),
    }
}
