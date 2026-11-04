//! Brainmatter speaks German or English, as macOS does: German if it comes first among
//! the languages in System Settings, English otherwise. `BM_LANG=de` or `en` overrides
//! it, for screenshots and tests.
//!
//! The web view gets the same choice before any of its scripts run (see [`script`]),
//! so the app never shows two languages at once.

use std::sync::OnceLock;

struct Language {
    german: bool,
    /// For dates and numbers, such as `de-DE` or `en-GB`.
    locale: String,
}

fn language() -> &'static Language {
    static LANGUAGE: OnceLock<Language> = OnceLock::new();
    LANGUAGE.get_or_init(|| {
        let preferred = bm_macos::locale::preferred_language().unwrap_or_else(|| "en-US".into());
        let german = match std::env::var("BM_LANG").as_deref() {
            Ok("de") => true,
            Ok("en") => false,
            _ => preferred.starts_with("de"),
        };
        let locale = match (german, preferred.starts_with("de")) {
            (true, true) | (false, false) => preferred,
            (true, false) => "de-DE".into(),
            (false, true) => "en-US".into(),
        };
        Language { german, locale }
    })
}

pub fn german() -> bool {
    language().german
}

/// The text in the app's language.
pub fn tr(de: &'static str, en: &'static str) -> &'static str {
    if german() { de } else { en }
}

/// Run in the web view before anything else.
pub fn script() -> String {
    let lang = if german() { "de" } else { "en" };
    format!(
        "window.__BM_LANG = {}; window.__BM_LOCALE = {};",
        serde_json::to_string(lang).expect("a string"),
        serde_json::to_string(&language().locale).expect("a string")
    )
}
