//! Templates and reusable prompts: Markdown entries with `{{placeholders}}`.

use std::collections::HashMap;

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// Calls `f` for every placeholder with its byte range and trimmed name.
fn scan(body: &str, mut f: impl FnMut(usize, usize, &str)) {
    let mut from = 0;
    while let Some(open) = body[from..].find("{{").map(|i| i + from) {
        let Some(close) = body[open + 2..].find("}}").map(|i| i + open + 2) else { break };
        let name = body[open + 2..close].trim();
        if !name.is_empty() && name.chars().all(is_name_char) {
            f(open, close + 2, name);
            from = close + 2;
        } else {
            // Not a placeholder, e.g. a code sample; look further from inside it.
            from = open + 2;
        }
    }
}

/// The placeholders of a template, each once, in order of appearance.
pub fn placeholders(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    scan(body, |_, _, name| {
        if !out.iter().any(|n| n == name) {
            out.push(name.to_string());
        }
    });
    out
}

/// Fills in the values. Placeholders without a value stay visible, and values
/// are inserted as they are: a value containing `{{x}}` is not expanded again.
pub fn render(body: &str, values: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(body.len());
    let mut copied = 0;
    scan(body, |start, end, name| {
        if let Some(value) = values.get(name) {
            out.push_str(&body[copied..start]);
            out.push_str(value);
            copied = end;
        }
    });
    out.push_str(&body[copied..]);
    out
}

/// A name usable as a command, e.g. in `/mcp__brainmatter__code-review`.
pub fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.to_lowercase().chars() {
        match c {
            'ä' => out.push_str("ae"),
            'ö' => out.push_str("oe"),
            'ü' => out.push_str("ue"),
            'ß' => out.push_str("ss"),
            c if c.is_ascii_alphanumeric() => out.push(c),
            _ if !out.is_empty() && !out.ends_with('-') => out.push('-'),
            _ => {}
        }
    }
    out.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_each_placeholder_once() {
        let body = "{{ name }} und {{ziel}}, wieder {{name}}";
        assert_eq!(placeholders(body), ["name", "ziel"]);
        assert!(placeholders("kein {{ }} und {{zwei worte}} und {{offen").is_empty());
    }

    #[test]
    fn code_samples_do_not_hide_later_placeholders() {
        assert_eq!(placeholders("{{#if x}} dann {{name}}"), ["name"]);
    }

    #[test]
    fn renders_values_and_keeps_the_rest() {
        let values = HashMap::from([("a".to_string(), "1".to_string())]);
        assert_eq!(render("{{a}}-{{ a }}-{{b}}", &values), "1-1-{{b}}");
    }

    #[test]
    fn values_are_not_expanded_again() {
        let values = HashMap::from([
            ("a".to_string(), "{{b}}".to_string()),
            ("b".to_string(), "geheim".to_string()),
        ]);
        assert_eq!(render("{{a}}", &values), "{{b}}");
    }

    #[test]
    fn handles_text_around_multibyte_characters() {
        let values = HashMap::from([("ä".to_string(), "ü".to_string())]);
        assert_eq!(render("ö {{ä}} ß", &values), "ö ü ß");
    }

    #[test]
    fn slugs_are_plain() {
        assert_eq!(slug("Code-Review Checkliste!"), "code-review-checkliste");
        assert_eq!(slug("  Übergabe für Größe "), "uebergabe-fuer-groesse");
        assert_eq!(slug("!!!"), "");
    }
}
