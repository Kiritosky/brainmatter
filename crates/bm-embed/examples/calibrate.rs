//! Measures passage-to-passage similarity for pairs of known relationship, to
//! choose the duplicate and link-suggestion thresholds from data.
//!
//! `cargo run --release -p bm-embed --example calibrate [-- <model dir>]`

use bm_core::semantic::Embedder;
use bm_embed::{E5Embedder, ModelStore};

const DUPLICATES: &[(&str, &str)] = &[
    (
        "Tokio\nAsynchrone Laufzeit für Rust mit Work-Stealing-Scheduler.",
        "Tokio Runtime\nTokio ist die asynchrone Rust-Laufzeitumgebung und nutzt einen Work-Stealing-Scheduler.",
    ),
    (
        "SQLCipher\nVerschlüsselt SQLite-Datenbanken seitenweise mit AES-256.",
        "SQLCipher\nEncrypts SQLite databases page by page using AES-256.",
    ),
    (
        "Argon2id\nSpeicherharte Funktion zum Hashen von Passwörtern.",
        "Argon2\nPasswort-Hashing, das viel Speicher braucht und so GPU-Angriffe erschwert.",
    ),
    (
        "Sauerteig ansetzen\nRoggenmehl und Wasser mischen, fünf Tage täglich füttern.",
        "Sauerteig-Starter\nMehl und Wasser verrühren und fünf Tage lang jeden Tag füttern.",
    ),
    (
        "Meeting mit Anna\nBudget für Q3 besprochen, Entscheidung nächste Woche.",
        "Besprechung Anna\nWir haben über das Q3-Budget gesprochen. Entscheidung fällt nächste Woche.",
    ),
    (
        "MCP\nModel Context Protocol: KIs rufen Werkzeuge über Server auf.",
        "Model Context Protocol\nProtokoll, mit dem KI-Assistenten Tools auf Servern aufrufen.",
    ),
    (
        "Kaffee kochen\nFilter mit 60 g Kaffee pro Liter, Wasser bei 94 Grad.",
        "Filterkaffee Rezept\n60 Gramm Kaffee auf einen Liter Wasser, 94 °C.",
    ),
    (
        "Rust Ownership\nJeder Wert hat genau einen Besitzer.",
        "Ownership in Rust\nEvery value has exactly one owner at a time.",
    ),
];

const RELATED: &[(&str, &str)] = &[
    (
        "Tokio\nAsynchrone Laufzeit für Rust.",
        "Axum\nWeb-Framework für Rust, aufgebaut auf Tokio und Hyper.",
    ),
    (
        "SQLCipher\nVerschlüsselt SQLite-Datenbanken mit AES-256.",
        "Argon2id\nLeitet aus dem Passwort den Schlüssel ab.",
    ),
    (
        "Sigma.js\nZeichnet große Graphen mit WebGL im Browser.",
        "ForceAtlas2\nKraftbasierter Layout-Algorithmus für Netzwerke.",
    ),
    (
        "Sauerteig\nRoggenmehl und Wasser, regelmäßig füttern.",
        "Roggenbrot backen\n70 % Roggen, mit Sauerteig über Nacht gehen lassen.",
    ),
    (
        "Marathon-Plan\n16 Wochen, lange Läufe am Sonntag.",
        "Laufschuhe\nNach 800 Kilometern austauschen, Dämpfung lässt nach.",
    ),
    (
        "Touch ID\nEntsperren per Fingerabdruck auf dem Mac.",
        "Keychain\nSpeichert Schlüssel und Passwörter unter macOS.",
    ),
    (
        "Rust Ownership\nJeder Wert hat einen Besitzer.",
        "Borrow Checker\nPrüft zur Compile-Zeit alle Referenzen.",
    ),
    (
        "Svelte 5\nRunes wie $state und $derived.",
        "SvelteKit\nApp-Framework mit Routing für Svelte.",
    ),
    (
        "PageRank\nBewertet Knoten nach der Wichtigkeit ihrer Nachbarn.",
        "Graph-Cluster\nLouvain findet zusammenhängende Gruppen im Netzwerk.",
    ),
    (
        "Steuererklärung\nBelege für Fahrtkosten und Arbeitszimmer sammeln.",
        "Arbeitszimmer absetzen\nPauschale von 1260 Euro pro Jahr beim Finanzamt.",
    ),
];

const UNRELATED: &[(&str, &str)] = &[
    (
        "Tokio\nAsynchrone Laufzeit für Rust.",
        "Sauerteig\nRoggenmehl und Wasser, regelmäßig füttern.",
    ),
    (
        "SQLCipher\nVerschlüsselt SQLite-Datenbanken.",
        "Marathon-Plan\n16 Wochen, lange Läufe am Sonntag.",
    ),
    ("Touch ID\nEntsperren per Fingerabdruck.", "Steuererklärung\nBelege für Fahrtkosten sammeln."),
    ("Sigma.js\nZeichnet Graphen mit WebGL.", "Kaffee kochen\n60 g Kaffee pro Liter."),
    (
        "Meeting mit Anna\nBudget für Q3 besprochen.",
        "Borrow Checker\nPrüft zur Compile-Zeit alle Referenzen.",
    ),
    (
        "Rust Ownership\nJeder Wert hat einen Besitzer.",
        "Urlaub Italien\nFlüge nach Neapel im Mai buchen.",
    ),
    (
        "Roggenbrot\n70 % Roggen, über Nacht gehen lassen.",
        "MCP\nKIs rufen Werkzeuge über Server auf.",
    ),
    (
        "Laufschuhe\nNach 800 Kilometern austauschen.",
        "Argon2id\nSpeicherharte Passwort-Hashfunktion.",
    ),
    (
        "Geburtstag Mama\nBlumen bestellen, Tisch reservieren.",
        "ForceAtlas2\nKraftbasierter Layout-Algorithmus.",
    ),
    ("Svelte 5\nRunes wie $state.", "Arbeitszimmer absetzen\nPauschale 1260 Euro."),
    (
        "Das Treffen wurde verschoben\nNeuer Termin am Dienstag.",
        "Der Build ist fehlgeschlagen\nFehler im Linker bei ONNX.",
    ),
    ("Projektidee\nApp für Rezepte bauen.", "Projektstatus\nDas Release verschiebt sich."),
];

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn report(e: &E5Embedder, label: &str, pairs: &[(&str, &str)]) -> Vec<f32> {
    let mut scores = Vec::new();
    for (a, b) in pairs {
        let v = e.embed_passages(&[a.to_string(), b.to_string()]).unwrap();
        scores.push(cosine(&v[0], &v[1]));
    }
    let mut sorted = scores.clone();
    sorted.sort_by(f32::total_cmp);
    let line: Vec<_> = sorted.iter().map(|s| format!("{s:.3}")).collect();
    println!("{label:10} {}", line.join(" "));
    scores
}

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("{}/../../target/models", env!("CARGO_MANIFEST_DIR")));
    let store = ModelStore::new(dir);
    store.download(|_| {}).expect("model");
    let e = E5Embedder::new(store);
    let dup = report(&e, "duplicate", DUPLICATES);
    let rel = report(&e, "related", RELATED);
    let unr = report(&e, "unrelated", UNRELATED);

    let f = |scores: &[f32]| scores.iter().copied().fold(f32::INFINITY, f32::min);
    let g = |scores: &[f32]| scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    println!(
        "min duplicate {:.3} | max related {:.3} | min related {:.3} | max unrelated {:.3}",
        f(&dup),
        g(&rel),
        f(&rel),
        g(&unr)
    );
    for t in [0.80, 0.82, 0.84, 0.86, 0.88, 0.90, 0.92, 0.94] {
        let dup_hit = dup.iter().filter(|s| **s >= t).count();
        let rel_hit = rel.iter().filter(|s| **s >= t).count();
        let unr_hit = unr.iter().filter(|s| **s >= t).count();
        println!(
            "t={t:.2}  duplicates {dup_hit}/{}  related {rel_hit}/{}  unrelated {unr_hit}/{}",
            dup.len(),
            rel.len(),
            unr.len()
        );
    }
}
