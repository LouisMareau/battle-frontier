//! The content tool:
//!   content check            reads both spaces, prints what's in them and every broken reference
//!   content bundle [space]   packs content/ (or temporary/) into its bundle.json for the browser
//!   content format           rewrites content/'s records (and chunk files) in the standard layout
//!   content chunk MAP        stores a map as chunk files (data/maps/MAP/<x>_<z>.json) instead of rows
//!   content unchunk MAP      back to rows of characters in the record
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
            each!(types, creatures, moves, abilities, items, natures, growth_rates, egg_groups, regions, encounters, parties, settings, terrains, props, maps, spawns, worlds, templates);
            let mut chunks = 0;
            for e in content.maps.entries().iter().filter(|e| e.space == Space::Content && e.value.chunked()) {
                match disk::read_chunks(&root, Space::Content, &e.value).and_then(|c| disk::save_chunks(&root, Space::Content, &e.value, &c)) {
                    Ok((written, _)) => chunks += written,
                    Err(err) => {
                        eprintln!("{err}");
                        std::process::exit(1);
                    }
                }
            }
            println!("content/: {n} records written, {chunks} chunk files rewritten");
        }
        Some(cmd @ ("chunk" | "unchunk")) => {
            let Some(id) = args.get(1) else {
                eprintln!("{cmd} MAP");
                std::process::exit(2);
            };
            let content = disk::load(&root);
            let Some(e) = content.maps.entry(id) else {
                eprintln!("no map {id}");
                std::process::exit(1);
            };
            let (space, m) = (e.space, &e.value);
            let result = if cmd == "chunk" {
                if m.chunked() {
                    Err(format!("{id} is already in chunk files"))
                } else {
                    let (header, chunks) = m.with_chunks(&m.cells(&[]));
                    disk::save(&root, space, &header)
                        .and_then(|_| disk::save_chunks(&root, space, &header, &chunks))
                        .map(|(written, _)| format!("{id}: {written} chunk files of {} cells in {}/data/maps/{id}/", header.chunk, space.dir()))
                }
            } else if !m.chunked() {
                Err(format!("{id} is rows already"))
            } else {
                disk::read_chunks(&root, space, m)
                    .and_then(|chunks| disk::save(&root, space, &m.with_rows(&m.cells(&chunks))))
                    .map(|path| format!("{}: rows again; {} chunk files removed", path.display(), disk::remove_chunks(&root, space, id)))
            };
            match result {
                Ok(msg) => println!("{msg}"),
                Err(err) => {
                    eprintln!("{err}");
                    std::process::exit(1);
                }
            }
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
            let mut problems = content.validate();
            problems.extend(disk::missing_files(&root, &content));
            problems.extend(disk::check_chunks(&root, &content));
            for note in disk::seam_notes(&root, &content) {
                println!("note: {note}");
            }
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
            eprintln!("unknown command {other}: use check, bundle, format, chunk MAP or unchunk MAP");
            std::process::exit(2);
        }
    }
}
