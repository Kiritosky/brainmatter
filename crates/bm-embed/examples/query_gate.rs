//! Measures how well different rules tell real search hits from false ones, on a
//! corpus shaped like a real vault: short notes next to long documents, which sit
//! close to almost any query once their chunks are averaged.
//!
//! `cargo run --release -p bm-embed --example query_gate [-- <model dir>]`

use bm_core::semantic::{Embedder, passages, pool};
use bm_embed::{E5Embedder, ModelStore};

const NOTES: &[&str] = &[
    "Sauerteig\nRoggenmehl und Wasser mischen, fünf Tage täglich füttern.",
    "Roggenbrot backen\n70 % Roggen, mit Sauerteig über Nacht gehen lassen.",
    "Tokio\nAsynchrone Laufzeit für Rust mit Work-Stealing-Scheduler.",
    "Axum\nWeb-Framework für Rust, aufgebaut auf Tokio und Hyper.",
    "Argon2id\nSpeicherharte Funktion zum Hashen von Passwörtern.",
    "Marathon-Plan\n16 Wochen, lange Läufe am Sonntag.",
    "Laufschuhe\nNach 800 Kilometern austauschen, Dämpfung lässt nach.",
    "Arbeitszimmer\nPauschale von 1260 Euro pro Jahr beim Finanzamt.",
    "SQLCipher\nEncrypts SQLite databases page by page using AES-256.",
    "Sigma.js\nRenders large networks with WebGL in the browser.",
    "ForceAtlas2\nKraftbasierter Layout-Algorithmus für Netzwerke.",
    "Meeting mit Anna\nBudget für Q3 besprochen, Entscheidung nächste Woche.",
    "MCP\nModel Context Protocol: KIs rufen Werkzeuge über Server auf.",
    "Touch ID\nEntsperren per Fingerabdruck auf dem Mac.",
    "Keychain\nSpeichert Schlüssel und Passwörter unter macOS.",
    "Rust Ownership\nJeder Wert hat genau einen Besitzer.",
    "Borrow Checker\nPrüft zur Compile-Zeit alle Referenzen.",
    "Svelte 5\nRunes wie $state und $derived.",
    "Kaffee kochen\nFilter mit 60 g Kaffee pro Liter, Wasser bei 94 Grad.",
    "Urlaub Italien\nFlüge nach Neapel im Mai buchen.",
    "Geburtstag Mama\nBlumen bestellen, Tisch reservieren.",
    "Steuererklärung\nBelege für Fahrtkosten und Arbeitszimmer sammeln.",
];

/// A search and the titles that count as right. Sections of PLAN.md, renamed to a
/// made-up product, stand in for the long documents of another project.
const FINDABLE: &[(&str, &[&str])] = &[
    ("Brot selber backen", &["Sauerteig", "Roggenbrot backen"]),
    ("async runtime", &["Tokio"]),
    ("Passwort sicher speichern", &["Argon2id", "Keychain", "5. Sicherheit"]),
    ("für den Wettkampf trainieren", &["Marathon-Plan"]),
    ("was kann ich von der Steuer absetzen", &["Arbeitszimmer", "Steuererklärung"]),
    ("Datenbank verschlüsseln", &["SQLCipher", "5. Sicherheit"]),
    ("Graph zeichnen", &["Sigma.js", "ForceAtlas2", "6. Graph-Engine, Suche, Intelligenz"]),
    ("Besprechung über Geld", &["Meeting mit Anna"]),
    ("how do AI tools talk to servers", &["MCP", "7. MCP-Schnittstelle (Herzstück)"]),
    ("Fingerabdruck entsperren", &["Touch ID"]),
    ("welche Risiken gibt es", &["12. Risiken"]),
    ("wie schnell muss die App starten", &["9. Performance-Ziele"]),
    ("Aufbau der Datenbank Tabellen", &["4. Datenmodell"]),
    ("Fahrtkosten", &["Steuererklärung"]),
    ("Italien Reise", &["Urlaub Italien"]),
];

/// Searches nothing in the corpus answers. Every hit is a false one.
const UNANSWERED: &[&str] = &[
    "Brainmatter",
    "Zephyrine",
    "Kubernetes Cluster skalieren",
    "Hochzeit planen",
    "Rezept für Lasagne",
    "Quantenphysik",
    "Fußball Bundesliga Ergebnisse",
    "wie repariere ich mein Fahrrad",
    "Steuerbescheid Einspruch Frist",
    "Docker Compose",
    "Kindergeburtstag Ideen",
    "invoice payment overdue",
];

/// A name, and which entries it lets through given raw and centred scores.
type Rule = (String, Box<dyn Fn(&(Vec<f32>, Vec<f32>)) -> Vec<usize>>);

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn unit(mut v: Vec<f32>) -> Vec<f32> {
    bm_core::semantic::normalize(&mut v);
    v
}

fn centred(v: &[f32], centre: &[f32]) -> Vec<f32> {
    unit(v.iter().zip(centre).map(|(a, b)| a - b).collect())
}

fn floor_margin(scores: &[f32], floor: f32, margin: f32) -> Vec<usize> {
    let best = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    (0..scores.len()).filter(|i| scores[*i] >= (best - margin).max(floor)).collect()
}

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("{}/../../target/models", env!("CARGO_MANIFEST_DIR")));
    let store = ModelStore::new(dir);
    store.download(|_| {}).expect("model");
    let e = E5Embedder::new(store);

    let plan = include_str!("../../../PLAN.md").replace("Brainmatter", "Solvane");
    let mut titles = Vec::new();
    let mut pooled = Vec::new();
    let mut chunked: Vec<Vec<Vec<f32>>> = Vec::new();
    let mut add = |title: &str, body: &str| {
        let vectors = e.embed_passages(&passages(title, body)).unwrap();
        titles.push(title.to_string());
        pooled.push(pool(&vectors));
        chunked.push(vectors);
    };
    for note in NOTES {
        let (title, body) = note.split_once('\n').unwrap();
        add(title, body);
    }
    for section in plan.split("\n## ").skip(1) {
        let (title, body) = section.split_once('\n').unwrap();
        add(title.trim(), body);
    }
    for text in [include_str!("../../../README.md"), include_str!("../../../CHANGELOG.md")] {
        for section in text.replace("Brainmatter", "Solvane").split("\n## ").skip(1) {
            let (title, body) = section.split_once('\n').unwrap();
            add(title.trim(), body);
        }
    }
    let long = chunked.iter().filter(|c| c.len() > 1).count();
    println!("{} entries, {long} of them long", titles.len());

    // How a rule holds up with the size and make-up of the vault.
    let notes: Vec<usize> = (0..NOTES.len()).collect();
    let everything: Vec<usize> = (0..titles.len()).collect();
    let plan_only = NOTES.len() + plan.split("\n## ").skip(1).count();
    let without_extra: Vec<usize> = (0..plan_only).collect();
    for (name, corpus) in
        [("notes only", &notes), ("notes + PLAN", &without_extra), ("everything", &everything)]
    {
        println!("\n=== {name}: {} entries", corpus.len());
        let dim = pooled[0].len();
        let mut centre = vec![0.0f32; dim];
        for i in corpus {
            for (c, x) in centre.iter_mut().zip(&pooled[*i]) {
                *c += x / corpus.len() as f32;
            }
        }
        let vectors: Vec<Vec<f32>> = corpus.iter().map(|i| centred(&pooled[*i], &centre)).collect();
        let plain: Vec<&Vec<f32>> = corpus.iter().map(|i| &pooled[*i]).collect();
        let score = |query: &str| {
            let q = e.embed_query(query).unwrap();
            let qc = centred(&q, &centre);
            (
                plain.iter().map(|v| cosine(&q, v)).collect::<Vec<f32>>(),
                vectors.iter().map(|v| cosine(&qc, v)).collect::<Vec<f32>>(),
            )
        };
        let findable: Vec<_> = FINDABLE
            .iter()
            .filter(|(_, right)| corpus.iter().any(|i| right.contains(&titles[*i].as_str())))
            .map(|(q, right)| (*right, score(q)))
            .collect();
        let unanswered: Vec<_> = UNANSWERED.iter().map(|q| score(q)).collect();
        let mut rules: Vec<Rule> = Vec::new();
        rules.push(("today".into(), Box::new(|s| floor_margin(&s.0, 0.80, 0.02))));
        for t in [0.14f32, 0.16, 0.18, 0.20, 0.22] {
            for m in [0.06f32, 0.10] {
                rules.push((
                    format!("centred {t:.2} / {m:.2}"),
                    Box::new(move |s| floor_margin(&s.1, t, m)),
                ));
            }
        }
        println!("{:24} {:>8} {:>11} {:>12}", "rule", "found", "false(hit)", "false(none)");
        for (name, rule) in &rules {
            let (mut found, mut wrong) = (0, 0);
            for (right, s) in &findable {
                let hits = rule(s);
                let is_right = |i: &usize| right.contains(&titles[corpus[*i]].as_str());
                found += usize::from(hits.iter().any(is_right));
                wrong += hits.iter().filter(|i| !is_right(i)).count();
            }
            let none: usize = unanswered.iter().map(|s| rule(s).len()).sum();
            println!("{name:24} {found:>5}/{} {wrong:>11} {none:>12}", findable.len());
        }
    }
}
