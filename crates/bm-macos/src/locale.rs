//! The languages the user set in System Settings.

use objc2_foundation::NSLocale;

/// The first of them, such as `de-DE` or `en-GB`. Unlike what the web view reports,
/// this does not depend on the languages the app bundle declares.
pub fn preferred_language() -> Option<String> {
    NSLocale::preferredLanguages().firstObject().map(|l| l.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn there_is_one() {
        let language = super::preferred_language().unwrap();
        assert!(language.len() >= 2, "{language}");
    }
}
