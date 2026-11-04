//! macOS integrations: Touch ID (LocalAuthentication), the Keychain and
//! sleep/screen-lock notifications, bookmarks and watching folders.

#![cfg(target_os = "macos")]

pub mod biometrics;
pub mod bookmarks;
pub mod keychain;
pub mod locale;
pub mod power;
pub mod watch;
