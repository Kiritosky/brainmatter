//! A vault full of invented entries, for measuring the real app at scale.
//!
//! `cargo run --release -p bm-core --example seed_vault -- <empty dir> <password> [50000]`
//!
//! Then `app/scripts/perf-app.sh` runs the app against it. Three links per entry, so
//! 50,000 entries come with about 150,000 links, in nested projects.

use std::time::Instant;

use bm_core::{Engine, Vault, model::*, vault::KdfParams};

const LINKS_PER_ENTRY: usize = 3;
// A capture takes at most 500 links.
const BATCH: usize = 160;
const TOP_PROJECTS: usize = 12;
const SUB_PROJECTS: usize = 3;
const WORDS_PER_BODY: usize = 60;

fn next(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

fn words(seed: &mut u64, count: usize) -> String {
    const SYLLABLES: &[&str] = &[
        "da", "ten", "bank", "ver", "bin", "dung", "pro", "jekt", "gra", "phen", "no", "tiz", "me",
        "mo", "ry", "ser", "ver", "such", "in", "dex", "lay", "out", "code", "test", "plan",
        "idee",
    ];
    let mut text = String::new();
    for _ in 0..count {
        if !text.is_empty() {
            text.push(' ');
        }
        for _ in 0..2 + next(seed) % 3 {
            text.push_str(SYLLABLES[(next(seed) % SYLLABLES.len() as u64) as usize]);
        }
    }
    text
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(dir), Some(password)) = (args.next(), args.next()) else {
        eprintln!("usage: seed_vault <empty dir> <password> [entries]");
        std::process::exit(2);
    };
    let total: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(50_000);

    let (vault, keys, _) =
        Vault::create(dir.as_ref(), &password, KdfParams::recommended()).unwrap();
    let engine = Engine::open(&vault.db_path(), &keys.db_key(), 4).unwrap();
    let user = Ctx::new(engine.ensure_actor(ActorKind::User, "Messung").unwrap().id);

    let mut projects = Vec::new();
    for t in 0..TOP_PROJECTS {
        let input = NewProject { name: format!("Bereich {t}"), ..Default::default() };
        let top = engine.create_project(&user, input).unwrap().id;
        for s in 0..SUB_PROJECTS {
            let input = NewProject {
                name: format!("Thema {t}.{s}"),
                parent: Some(top.clone()),
                ..Default::default()
            };
            projects.push(engine.create_project(&user, input).unwrap().id);
        }
        projects.push(top);
    }

    let started = Instant::now();
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut created = 0;
    while created < total {
        let size = BATCH.min(total - created);
        let nodes = (0..size)
            .map(|i| CaptureNode {
                key: Some(format!("k{i}")),
                node: NewNode {
                    title: format!("{} {}", words(&mut seed, 3), created + i),
                    body: words(&mut seed, WORDS_PER_BODY),
                    ..Default::default()
                },
            })
            .collect();
        let links = (0..size * LINKS_PER_ENTRY)
            .map(|_| CaptureLink {
                from: format!("k{}", next(&mut seed) % size as u64),
                to: format!("k{}", next(&mut seed) % size as u64),
                relation: None,
                note: None,
            })
            .filter(|l| l.from != l.to)
            .collect();
        let project = Some(projects[created / BATCH % projects.len()].clone());
        engine.capture(&user, Capture { nodes, links, project }).unwrap();
        created += size;
    }
    println!("{total} entries in {} projects, {:.1?}", projects.len(), started.elapsed());
}
