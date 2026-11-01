//! macOS integrations: Touch ID (LocalAuthentication), the Keychain and
//! sleep/screen-lock notifications.

#![cfg(target_os = "macos")]

pub mod biometrics;
pub mod bookmarks;
pub mod keychain;
pub mod power;
