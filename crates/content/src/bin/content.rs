//! The content tool:
//!   content check            reads both spaces, prints what's in them and every broken reference
//!   content bundle [space]   packs content/ (or temporary/) into its bundle.json for the browser
//!   content format           rewrites content/'s records in the standard layout (as the editor saves)
//! Run from the project folder (or set GAME_ROOT).

use std::path::PathBuf;

use content::{disk, Space};

fn main() {
    let root = PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("bundle") => {
            let spaces: Vec<Space> = match args.get(1).map(String::as_str) {
                Some("content") => vec![Space::Content],
                Some("temporary") => vec![Space::Temporary],
                _ => vec![Space::Content, Space::Temporary],
            };
            for space in spaces {
                let dir = root.join(space.dir());
                if !dir.exists() {
                    continue;
                }
                match disk::write_bundle(&dir, space) {
                    Ok(n) => println!("{}/bundle.json: {n} records", space.dir()),
                    Err(e) => {
                        eprintln!("{}: {e}", space.dir());
                        std::process::exit(1);
                    }
                }
            }
        }
        Some("format") => {
            let content = disk::load(&root);
            let mut n = 0;
            macro_rules! each {
                ($($field:ident),*) => {$(
                    for e in content.$field.entries().iter().filter(|e| e.space == Space::Content) {
                        if let Err(err) = disk::save(&root, Space::Content, &e.value) {
                            eprintln!("{err}");
                            std::process::exit(1);
                        }
                        n += 1;
                    }
                )*};
            }
            each!(types, creatures, moves, abilities, items, natures, growth_rates, egg_groups, regions, encounters, parties, settings, terrains, props, maps, spawns);
            println!("content/: {n} records written");
        }
        Some("check") | None => {
            let content = disk::load(&root);
            let mut last = "";
            for ((collection, space), n) in content.counts() {
                if collection != last {
                    print!("\n{collection:>13}:");
                    last = collection;
                }
                print!(" {n} {}", space.dir());
            }
            println!();
            let problems = content.validate();
            if problems.is_empty() {
                println!("no problems");
            } else {
                for p in &problems {
                    println!("problem: {p}");
                }
                println!("{} problems", problems.len());
                std::process::exit(1);
            }
        }
        Some(other) => {
            eprintln!("unknown command {other}: use check, bundle or format");
            std::process::exit(2);
        }
    }
}
