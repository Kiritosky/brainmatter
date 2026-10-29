//! Shared corpus and helpers for the embedding benchmarks.

pub const NOTES: &[&str] = &[
    "Async Runtime: Tokio ist eine asynchrone Laufzeitumgebung für Rust mit Work-Stealing-Scheduler.",
    "SQLCipher encrypts SQLite databases page by page with AES-256.",
    "Argon2id ist eine speicherintensive Passwort-Hashfunktion gegen GPU-Angriffe.",
    "Sigma.js renders large graphs in the browser using WebGL.",
    "Sauerteigbrot braucht einen aktiven Anstellgut und lange Gare.",
    "The Model Context Protocol lets AI assistants call tools exposed by servers.",
    "Touch ID wird über das LocalAuthentication-Framework von macOS abgefragt.",
    "PageRank ranks nodes in a graph by the importance of nodes linking to them.",
    "Marathon-Training: lange Läufe am Wochenende, Intervalle unter der Woche.",
    "Borrow checker: Rust verhindert Data Races zur Compile-Zeit durch Ownership.",
    "ForceAtlas2 is a force-directed layout algorithm for network visualization.",
    "Steuererklärung: Belege für Arbeitszimmer und Fahrtkosten sammeln.",
];

/// (query, index of the note that should rank first)
pub const QUERIES: &[(&str, usize)] = &[
    ("tokio Nebenläufigkeit", 0),
    ("concurrency runtime for async rust", 0),
    ("Datenbank verschlüsseln", 1),
    ("password hashing", 2),
    ("Graph im Browser zeichnen", 3),
    ("bread baking", 4),
    ("KI Werkzeuge anbinden", 5),
    ("fingerprint unlock on mac", 6),
    ("wichtigste Knoten im Netzwerk finden", 7),
    ("Laufen Trainingsplan", 8),
    ("memory safety without garbage collector", 9),
    ("graph layout algorithm", 10),
    ("tax return receipts", 11),
];

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (na * nb)
}

pub fn rss_mb() -> u64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().parse::<u64>().unwrap_or(0) / 1024
}
