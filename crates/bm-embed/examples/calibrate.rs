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

/// A search and the passage it is looking for, in other words.
const QUERIES: &[(&str, &str)] = &[
    ("Brot selber backen", "Sauerteig\nRoggenmehl und Wasser mischen, fünf Tage täglich füttern."),
    ("async runtime", "Tokio\nAsynchrone Laufzeit für Rust mit Work-Stealing-Scheduler."),
    ("Passwort sicher speichern", "Argon2id\nSpeicherharte Funktion zum Hashen von Passwörtern."),
    ("für den Wettkampf trainieren", "Marathon-Plan\n16 Wochen, lange Läufe am Sonntag."),
    (
        "was kann ich von der Steuer absetzen",
        "Arbeitszimmer\nPauschale von 1260 Euro pro Jahr beim Finanzamt.",
    ),
    ("Datenbank verschlüsseln", "SQLCipher\nEncrypts SQLite databases page by page using AES-256."),
    ("Graph zeichnen", "Sigma.js\nRenders large networks with WebGL in the browser."),
    (
        "Besprechung über Geld",
        "Meeting mit Anna\nBudget für Q3 besprochen, Entscheidung nächste Woche.",
    ),
    (
        "how do AI tools talk to servers",
        "MCP\nModel Context Protocol: KIs rufen Werkzeuge über Server auf.",
    ),
    ("Fingerabdruck entsperren", "Touch ID\nEntsperren per Fingerabdruck auf dem Mac."),
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

    // Query-to-passage scores are lower than passage-to-passage ones, so they
    // need their own threshold. Every other query's passage counts as a miss.
    let passages: Vec<String> = QUERIES.iter().map(|(_, p)| p.to_string()).collect();
    let vectors = e.embed_passages(&passages).unwrap();
    let mut right = Vec::new();
    let mut wrong = Vec::new();
    let mut margins = Vec::new();
    for (i, (query, _)) in QUERIES.iter().enumerate() {
        let q = e.embed_query(query).unwrap();
        let scores: Vec<f32> = vectors.iter().map(|v| cosine(&q, v)).collect();
        let others = scores.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, s)| *s);
        let best_other = others.clone().fold(f32::NEG_INFINITY, f32::max);
        println!("{query:40} right {:.3} | best wrong {best_other:.3}", scores[i]);
        right.push(scores[i]);
        wrong.extend(others);
        margins.push(scores[i] - best_other);
    }
    right.sort_by(f32::total_cmp);
    wrong.sort_by(f32::total_cmp);
    println!("right {:?}", right.iter().map(|s| format!("{s:.3}")).collect::<Vec<_>>());
    for t in [0.78, 0.80, 0.81, 0.82, 0.83, 0.84, 0.85, 0.86, 0.88] {
        let r = right.iter().filter(|s| **s >= t).count();
        let w = wrong.iter().filter(|s| **s >= t).count();
        println!("query t={t:.2}  right {r}/{}  wrong {w}/{}", right.len(), wrong.len());
    }

    // Absolute floor plus a distance to the best hit of the same query.
    for (floor, margin) in [(0.80, 0.03), (0.81, 0.03), (0.80, 0.02), (0.81, 0.02), (0.79, 0.03)] {
        let (mut r, mut w, mut first) = (0, 0, 0);
        for (i, (query, _)) in QUERIES.iter().enumerate() {
            let q = e.embed_query(query).unwrap();
            let scores: Vec<f32> = vectors.iter().map(|v| cosine(&q, v)).collect();
            let best = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let cut = (best - margin).max(floor);
            r += usize::from(scores[i] >= cut);
            w += scores.iter().enumerate().filter(|(j, s)| *j != i && **s >= cut).count();
            first += usize::from(scores[i] == best);
        }
        println!(
            "floor {floor:.2} margin {margin:.2}: right {r}/10 wrong {w}/90 (right is best: {first}/10)"
        );
    }

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
