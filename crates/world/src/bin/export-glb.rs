//! glTF files for Blender (and anything else that reads glTF):
//!   export-glb prop ID          one prop, at the origin (its stand-in, or its own model)
//!   export-glb props            every prop, each its own file, and all of them laid out
//!                               by category in props.glb
//!   export-glb map ID           a whole map: ground, props and trees, buildings
//!   export-glb building MAP ID  one building of a map
//!   export-glb check FILE       reads a .glb as the engine does and says what's in it
//! Files go to build/exports/ (or --out DIR). Run from the project folder (or set GAME_ROOT).

use std::path::PathBuf;

use world::gltf_in::{extras_note, load_disk_models, load_model};
use world::gltf_out::{building_glb, map_glb, prop_glb, props_glb};

fn main() {
    let root = PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let out = match args.iter().position(|a| a == "--out") {
        Some(i) => {
            let dir = args.get(i + 1).cloned().unwrap_or_default();
            args.drain(i..(i + 2).min(args.len()));
            PathBuf::from(dir)
        }
        None => root.join("build").join("exports"),
    };
    let fail = |msg: String| -> ! {
        eprintln!("{msg}");
        std::process::exit(1)
    };
    if args.first().map(String::as_str) == Some("check") {
        let file = args.get(1).unwrap_or_else(|| fail("check FILE".into()));
        let bytes = std::fs::read(file).unwrap_or_else(|e| fail(format!("{file}: {e}")));
        let m = load_model(&bytes, file).unwrap_or_else(|e| fail(format!("{file}: {e}")));
        // a rigged model's triangles are its skinned parts' (none of them in the still scene)
        let skinned: usize = m.skinned.iter().map(|s| s.mesh.indices.len() / 3).sum();
        let tris = m.scene.plain.indices.len() / 3 + m.scene.textured.values().map(|t| t.indices.len() / 3).sum::<usize>() + skinned;
        let tris = if skinned > 0 { format!("{tris} triangles ({skinned} of them skinned)") } else { format!("{tris} triangles") };
        let size = m.max - m.min;
        let note = extras_note(&m);
        println!("{file}: {tris}, {} textures, {:.2} x {:.2} x {:.2} (width, height, depth), from {:?} to {:?}{}", m.images.len(), size.x, size.y, size.z, m.min.to_array(), m.max.to_array(), if note.is_empty() { String::new() } else { format!("; {note}") });
        for l in &m.lights {
            println!("  light: {} {} at {:?} aiming {:?}, intensity {}, range {}, cone {}", l.kind, l.color, l.at, l.aim, l.intensity, l.range, l.cone);
        }
        return;
    }
    let content = content::disk::load(&root);
    let (models, problems) = load_disk_models(&root, &content);
    for p in &problems {
        eprintln!("model not loaded: {p}");
    }
    let lh = content.world().level_height() as f32;
    std::fs::create_dir_all(&out).unwrap_or_else(|e| fail(format!("{}: {e}", out.display())));
    let write = |name: String, bytes: Vec<u8>| {
        let path = out.join(name);
        std::fs::write(&path, &bytes).unwrap_or_else(|e| fail(format!("{}: {e}", path.display())));
        println!("{} ({} KB)", path.display(), bytes.len() / 1024);
    };
    let map = |id: &str| world::load_map(&root, &content, id).unwrap_or_else(|e| fail(e));
    match (args.first().map(String::as_str), args.get(1), args.get(2)) {
        (Some("prop"), Some(id), _) => {
            let p = content.props.get(id).unwrap_or_else(|| fail(format!("no prop {id}")));
            write(format!("{id}.glb"), prop_glb(p, &models));
        }
        (Some("props"), _, _) => {
            for p in content.props.iter() {
                write(format!("{}.glb", p.id), prop_glb(p, &models));
            }
            write("props.glb".into(), props_glb(&content, &models));
        }
        (Some("map"), Some(id), _) => {
            let m = map(id);
            write(format!("{id}.glb"), map_glb(&m, &content, lh, &models));
        }
        (Some("building"), Some(m), Some(id)) => {
            let w = map(m);
            let b = w.buildings.iter().find(|b| &b.id == id).unwrap_or_else(|| fail(format!("no building {id} on {m}")));
            write(format!("{m}-{id}.glb"), building_glb(b, &content, lh, &models));
        }
        _ => fail("use: export-glb prop ID | props | map ID | building MAP ID | check FILE  [--out DIR]".into()),
    }
}
