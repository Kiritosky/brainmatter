//! macOS integrations: Touch ID (LocalAuthentication) and the Keychain.

#![cfg(target_os = "macos")]

pub mod biometrics;
pub mod keychain;
