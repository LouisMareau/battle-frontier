//! What the browser keeps of the editor's saves: each file at its place in the project
//! ("content/data/maps/meadow.json", a map's square, a model), in the browser's own storage
//! (IndexedDB), so saving asks nothing. Laid over the site's content when the page is opened
//! again (a record the site has just the same is dropped: the game has it now); Publish sends
//! them as a pull request; the editor's "Saved in this browser" lists them, downloads them as
//! one zip, or forgets them. With `?saves=downloads` in the address (the checks that read
//! downloads), or a browser that keeps nothing, saving downloads each file instead. The model
//! requests' folders are kept here too (private/requests/: their window reads and writes
//! them as the desktop's does the project's), never laid over the game nor published.

// what's here is the browser's (the desktop saves into the project's folders); the tests use
// the parts that aren't
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use std::collections::BTreeMap;
use std::sync::Arc;

use content::{Content, Space};

/// A file kept: its bytes, or that the editor removed it (a record deleted).
#[derive(Clone, Debug, PartialEq)]
pub enum Kept {
    File(Arc<Vec<u8>>),
    Removed,
}

/// Whether the browser keeps the editor's saves: still reading what it kept, yes, or no (and
/// why: saving downloads each file then).
#[derive(Clone, Debug, PartialEq)]
pub enum Keeping {
    Loading,
    Ready,
    No(String),
}

/// The record a file under a space's data/ holds ("content/data/maps/meadow.json": its space
/// and "maps/meadow.json"); None for any other file (a map's square, a model).
pub fn record_of(path: &str) -> Option<(Space, &str)> {
    let space = Space::ALL.into_iter().find(|s| path.strip_prefix(s.dir()).is_some_and(|rest| rest.starts_with("/data/")))?;
    let rel = &path[space.dir().len() + "/data/".len()..];
    let mut parts = rel.split('/');
    let (collection, file) = (parts.next()?, parts.next()?);
    (parts.next().is_none() && content::COLLECTIONS.contains(&collection) && file.ends_with(".json")).then_some((space, rel))
}

/// The kept files laid over the content just read from the site: records put in (or taken
/// out), the other files handed to `files` (a map's squares, models). The records the site has
/// just the same are left as they are and returned, to be forgotten: the game has them now.
pub fn lay_over(content: &mut Content, files: &crate::platform::Files, kept: &BTreeMap<String, Kept>) -> Vec<String> {
    let texts: Vec<(Space, BTreeMap<String, String>)> = Space::ALL.into_iter().map(|s| (s, content.space_texts(s))).collect();
    let site = |space: Space, rel: &str| texts.iter().find(|(s, _)| *s == space).and_then(|(_, t)| t.get(rel)).cloned();
    let mut same = vec![];
    for (path, k) in kept {
        match (record_of(path), k) {
            (Some((space, rel)), Kept::File(bytes)) => {
                let text = String::from_utf8_lossy(bytes);
                if site(space, rel).is_some_and(|t| t == text) {
                    same.push(path.clone());
                    continue;
                }
                let added = serde_json::from_str::<serde_json::Value>(&text).map_err(|e| format!("{rel}: {e}")).and_then(|v| content.add(space, rel, v));
                if let Err(e) = added {
                    content.problems.push(format!("kept in this browser: {e}"));
                }
            }
            // removed in the editor: taken out (a removal of what the site hasn't is no more)
            (Some((_, rel)), Kept::Removed) => {
                if !content.remove_file(rel) {
                    same.push(path.clone());
                }
            }
            // (the model requests' own files aren't the game's: their window reads them here)
            (None, Kept::File(_)) if path.starts_with("private/") => {}
            (None, Kept::File(bytes)) => files.insert(path, bytes.to_vec()),
            (None, Kept::Removed) => {}
        }
    }
    content.sort();
    same
}

/// The kept files as one zip, at their places in the project (stored as they are: they're
/// mostly small texts); the removed ones left out.
pub fn zip(kept: &BTreeMap<String, Kept>) -> Vec<u8> {
    let (mut out, mut central, mut count) = (vec![], vec![], 0u16);
    for (path, k) in kept {
        let Kept::File(bytes) = k else { continue };
        let (name, size, crc, at) = (path.as_bytes(), bytes.len() as u32, crc32fast::hash(bytes), out.len() as u32);
        // what the file's header and its entry in the list at the end share: the version
        // needed, the names in UTF-8, stored, 1980-01-01, its CRC and sizes, its name's length
        let mut common = vec![];
        for half in [20u16, 0x0800, 0, 0, 0x21] {
            common.extend_from_slice(&half.to_le_bytes());
        }
        for word in [crc, size, size] {
            common.extend_from_slice(&word.to_le_bytes());
        }
        common.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&common);
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(bytes);
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&common);
        // no extra field, no comment, the first disk, no attributes; where its header is
        for half in [0u16; 4] {
            central.extend_from_slice(&half.to_le_bytes());
        }
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&at.to_le_bytes());
        central.extend_from_slice(name);
        count += 1;
    }
    let (at, len) = (out.len() as u32, central.len() as u32);
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    for half in [0u16, 0, count, count] {
        out.extend_from_slice(&half.to_le_bytes());
    }
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&at.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[cfg(target_arch = "wasm32")]
pub use store::{files, forget, forget_all, get, keep, keep_removed, keeping, note, start, take_trouble, under};

/// The browser's IndexedDB: the database "battle-frontier", its store "files", each value
/// [path, bytes] (null bytes: removed) under its path; and what's in it, held here too.
#[cfg(target_arch = "wasm32")]
mod store {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;

    use super::{Keeping, Kept};

    const DB: &str = "battle-frontier";
    const STORE: &str = "files";

    struct State {
        keeping: Keeping,
        /// when the page began reading the storage (ms)
        started: f64,
        files: BTreeMap<String, Kept>,
        db: Option<web_sys::IdbDatabase>,
        /// what went wrong writing, for the editor to say
        trouble: Option<String>,
        /// the browser asked once to keep the site's storage even when it runs short
        asked: bool,
    }

    thread_local! {
        static STATE: RefCell<State> = const { RefCell::new(State { keeping: Keeping::No(String::new()), started: 0.0, files: BTreeMap::new(), db: None, trouble: None, asked: false }) };
    }

    fn js(e: JsValue) -> String {
        e.as_string().unwrap_or_else(|| format!("{e:?}"))
    }

    /// A request's answer, awaited (its handlers set at once, before the browser answers).
    fn answered(req: &web_sys::IdbRequest) -> JsFuture {
        let promise = js_sys::Promise::new(&mut |resolve, reject| {
            req.set_onsuccess(Some(&resolve));
            req.set_onerror(Some(&reject));
        });
        JsFuture::from(promise)
    }

    /// A transaction's end (written), awaited.
    fn committed(tx: &web_sys::IdbTransaction) -> JsFuture {
        let promise = js_sys::Promise::new(&mut |resolve, reject| {
            tx.set_oncomplete(Some(&resolve));
            tx.set_onerror(Some(&reject));
            tx.set_onabort(Some(&reject));
        });
        JsFuture::from(promise)
    }

    async fn open() -> Result<web_sys::IdbDatabase, String> {
        let factory = web_sys::window().and_then(|w| w.indexed_db().ok().flatten()).ok_or("this browser has no storage for sites (IndexedDB)")?;
        let req = factory.open_with_u32(DB, 1).map_err(js)?;
        let upgrade = Closure::<dyn FnMut(web_sys::IdbVersionChangeEvent)>::new(|e: web_sys::IdbVersionChangeEvent| {
            let db = e.target().and_then(|t| t.dyn_into::<web_sys::IdbOpenDbRequest>().ok()).and_then(|r| r.result().ok()).and_then(|v| v.dyn_into::<web_sys::IdbDatabase>().ok());
            if let Some(db) = db {
                if !db.object_store_names().contains(STORE) {
                    let _ = db.create_object_store(STORE);
                }
            }
        });
        req.set_onupgradeneeded(Some(upgrade.as_ref().unchecked_ref()));
        let opened = answered(&req).await;
        req.set_onupgradeneeded(None);
        drop(upgrade);
        opened.map_err(js)?;
        req.result().map_err(js)?.dyn_into::<web_sys::IdbDatabase>().map_err(|_| "the storage didn't open".to_string())
    }

    async fn read(db: &web_sys::IdbDatabase) -> Result<BTreeMap<String, Kept>, String> {
        let tx = db.transaction_with_str(STORE).map_err(js)?;
        let req = tx.object_store(STORE).map_err(js)?.get_all().map_err(js)?;
        answered(&req).await.map_err(js)?;
        let all: js_sys::Array = req.result().map_err(js)?.dyn_into().map_err(|_| "the storage's files aren't a list".to_string())?;
        let mut out = BTreeMap::new();
        for v in all.iter() {
            let Ok(pair) = v.dyn_into::<js_sys::Array>() else { continue };
            let Some(path) = pair.get(0).as_string() else { continue };
            let data = pair.get(1);
            let kept = if data.is_null() { Kept::Removed } else { Kept::File(Arc::new(js_sys::Uint8Array::new(&data).to_vec())) };
            out.insert(path, kept);
        }
        Ok(out)
    }

    /// Whether saving downloads each file (`?saves=downloads` in the address): nothing kept.
    pub fn downloads() -> bool {
        web_sys::window().and_then(|w| w.location().search().ok()).is_some_and(|q| q.trim_start_matches('?').split('&').any(|p| p == "saves=downloads"))
    }

    /// The page opening: what this browser kept read (the page waits for it before the
    /// content is up), unless saving downloads.
    pub fn start(ctx: &egui::Context) {
        if downloads() {
            STATE.with(|s| s.borrow_mut().keeping = Keeping::No("saving downloads each file here (?saves=downloads)".into()));
            return;
        }
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            (s.keeping, s.started) = (Keeping::Loading, js_sys::Date::now());
        });
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let got = match open().await {
                Ok(db) => read(&db).await.map(|files| (db, files)),
                Err(e) => Err(e),
            };
            STATE.with(|s| {
                let mut s = s.borrow_mut();
                // given up on already (it took too long): the page went on without
                if s.keeping != Keeping::Loading {
                    return;
                }
                match got {
                    Ok((db, files)) => (s.db, s.files, s.keeping) = (Some(db), files, Keeping::Ready),
                    Err(e) => {
                        log::warn!("the editor's saves aren't kept in this browser: {e}");
                        s.keeping = Keeping::No(e);
                    }
                }
            });
            ctx.request_repaint();
        });
    }

    pub fn keeping() -> Keeping {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            // the storage not answering (another tab holding it, say): the page goes on
            // without it, saving downloads
            if s.keeping == Keeping::Loading && js_sys::Date::now() - s.started > 8000.0 {
                log::warn!("the browser's storage didn't answer: saving downloads each file");
                s.keeping = Keeping::No("the browser's storage didn't answer".into());
            }
            s.keeping.clone()
        })
    }

    /// What's kept (and, when saving downloads, what was saved since the page opened).
    pub fn files() -> BTreeMap<String, Kept> {
        STATE.with(|s| s.borrow().files.clone())
    }

    /// A file kept here: its bytes (None: not kept, or removed).
    pub fn get(path: &str) -> Option<Arc<Vec<u8>>> {
        STATE.with(|s| match s.borrow().files.get(path) {
            Some(Kept::File(b)) => Some(b.clone()),
            _ => None,
        })
    }

    /// The files kept under `prefix` (a folder's, its path and a slash), in order.
    pub fn under(prefix: &str) -> Vec<String> {
        STATE.with(|s| {
            let s = s.borrow();
            s.files.range(prefix.to_string()..).take_while(|(p, _)| p.starts_with(prefix)).filter(|(_, k)| matches!(k, Kept::File(_))).map(|(p, _)| p.clone()).collect()
        })
    }

    /// What went wrong writing since it was last asked.
    pub fn take_trouble() -> Option<String> {
        STATE.with(|s| s.borrow_mut().trouble.take())
    }

    /// A file put in the storage (`Some`) or taken out of it (`None`): its own transaction,
    /// made at once, so they're written in the order asked; what goes wrong is told later.
    fn write(path: &str, kept: Option<&Kept>) {
        let Some(db) = STATE.with(|s| s.borrow().db.clone()) else { return };
        let key = JsValue::from_str(path);
        let made = (|| -> Result<web_sys::IdbTransaction, JsValue> {
            let tx = db.transaction_with_str_and_mode(STORE, web_sys::IdbTransactionMode::Readwrite)?;
            let store = tx.object_store(STORE)?;
            match kept {
                Some(k) => {
                    let data: JsValue = match k {
                        Kept::File(b) => js_sys::Uint8Array::from(&b[..]).into(),
                        Kept::Removed => JsValue::NULL,
                    };
                    store.put_with_key(&js_sys::Array::of2(&key, &data), &key)?;
                }
                None => {
                    store.delete(&key)?;
                }
            }
            Ok(tx)
        })();
        let trouble = |e: JsValue, path: &str| {
            let said = format!("{path} isn't kept in this browser: {}", js(e));
            log::warn!("{said}");
            STATE.with(|s| s.borrow_mut().trouble = Some(said));
        };
        match made {
            Ok(tx) => {
                let done = committed(&tx);
                let path = path.to_string();
                wasm_bindgen_futures::spawn_local(async move {
                    if let Err(e) = done.await {
                        trouble(e, &path);
                    }
                });
            }
            Err(e) => trouble(e, path),
        }
    }

    /// A file saved: kept (written when it's changed). Whether it changed.
    pub fn keep(path: &str, bytes: Vec<u8>) -> bool {
        let kept = Kept::File(Arc::new(bytes));
        let (changed, ask) = STATE.with(|s| {
            let mut s = s.borrow_mut();
            let changed = s.files.get(path) != Some(&kept);
            if changed {
                s.files.insert(path.to_string(), kept.clone());
            }
            (changed, changed && !std::mem::replace(&mut s.asked, true))
        });
        if changed {
            write(path, Some(&kept));
        }
        // the first time: the browser asked to keep the site's storage when it runs short of
        // room (Chrome decides by itself; another may ask)
        if ask {
            if let Some(w) = web_sys::window() {
                if let Ok(p) = w.navigator().storage().persist() {
                    wasm_bindgen_futures::spawn_local(async move {
                        let _ = JsFuture::from(p).await;
                    });
                }
            }
        }
        changed
    }

    /// A file saved while saving downloads: noted for Publish (the page's own), not kept.
    pub fn note(path: &str, bytes: Vec<u8>) {
        STATE.with(|s| s.borrow_mut().files.insert(path.to_string(), Kept::File(Arc::new(bytes))));
    }

    /// A record removed in the editor: kept so (the site's own taken out when the page opens).
    pub fn keep_removed(path: &str) {
        let changed = STATE.with(|s| s.borrow_mut().files.insert(path.to_string(), Kept::Removed) != Some(Kept::Removed));
        if changed {
            write(path, Some(&Kept::Removed));
        }
    }

    /// A kept file forgotten: the site's own again.
    pub fn forget(path: &str) {
        if STATE.with(|s| s.borrow_mut().files.remove(path).is_some()) {
            write(path, None);
        }
    }

    /// Everything kept forgotten; `then` once it's written (the page opened again, say).
    pub fn forget_all(then: impl FnOnce() + 'static) {
        let db = STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.files.clear();
            s.db.clone()
        });
        let cleared = db.and_then(|db| {
            let tx = db.transaction_with_str_and_mode(STORE, web_sys::IdbTransactionMode::Readwrite).ok()?;
            tx.object_store(STORE).ok()?.clear().ok()?;
            Some(committed(&tx))
        });
        wasm_bindgen_futures::spawn_local(async move {
            if let Some(done) = cleared {
                if let Err(e) = done.await {
                    log::warn!("the browser's saves aren't all forgotten: {}", js(e));
                }
            }
            then();
        });
    }
}

#[cfg(test)]
mod tests {
    use content::{Map, Settings, Template};

    use super::*;

    fn file(text: &str) -> Kept {
        Kept::File(Arc::new(text.as_bytes().to_vec()))
    }

    #[test]
    fn a_record_is_a_file_right_under_its_collection() {
        assert_eq!(record_of("content/data/maps/meadow.json"), Some((Space::Content, "maps/meadow.json")));
        assert_eq!(record_of("temporary/data/settings/world.json"), Some((Space::Temporary, "settings/world.json")));
        assert_eq!(record_of("content/data/maps/meadow/0_1.json"), None, "a map's square");
        assert_eq!(record_of("content/assets/models/rock.glb"), None, "a model");
        assert_eq!(record_of("content/data/nothing/x.json"), None, "no such collection");
        assert_eq!(record_of("contents/data/maps/x.json"), None);
    }

    /// What the site has, then what this browser kept laid over it: a map changed, the world
    /// settings just as the site has them (forgotten), a template removed, a removal of what
    /// the site hasn't (forgotten), a square and a model handed to the files.
    #[test]
    fn what_was_kept_laid_over_the_sites_content() {
        let mut c = Content::default();
        c.maps.put(Map { id: "meadow".into(), name: "Meadow".into(), width: 4, height: 4, ..Default::default() }, Space::Content);
        c.settings.put(Settings { id: "world".into(), ..Default::default() }, Space::Content);
        c.templates.put(Template { id: "hut".into(), ..Default::default() }, Space::Content);
        let site_world = content::to_text(c.settings.get("world").unwrap());
        let changed = Map { id: "meadow".into(), name: "Meadow by the pond".into(), width: 4, height: 4, ..Default::default() };
        let mut kept = BTreeMap::new();
        kept.insert("content/data/maps/meadow.json".to_string(), file(&content::to_text(&changed)));
        kept.insert("content/data/settings/world.json".to_string(), file(&site_world));
        kept.insert("content/data/templates/hut.json".to_string(), Kept::Removed);
        kept.insert("content/data/templates/shed.json".to_string(), Kept::Removed);
        kept.insert("content/data/maps/meadow/0_0.json".to_string(), file("{}\n"));
        kept.insert("content/assets/models/rock.glb".to_string(), file("glTF"));
        let files = crate::platform::Files::new("nowhere");
        let same = lay_over(&mut c, &files, &kept);
        assert_eq!(c.maps.get("meadow").map(|m| m.name.as_str()), Some("Meadow by the pond"));
        assert_eq!(c.maps.entry("meadow").map(|e| e.space), Some(Space::Content));
        assert!(c.templates.get("hut").is_none(), "removed");
        assert_eq!(same, ["content/data/settings/world.json", "content/data/templates/shed.json"]);
        assert!(matches!(files.get("content/data/maps/meadow/0_0.json"), crate::platform::Load::Ready(b) if *b == b"{}\n".to_vec()));
        assert!(matches!(files.get("content/assets/models/rock.glb"), crate::platform::Load::Ready(b) if *b == b"glTF".to_vec()));
        // a kept record that doesn't read: said, the site's own kept
        let mut bad = BTreeMap::new();
        bad.insert("content/data/maps/meadow.json".to_string(), file("{ not json"));
        let mut c2 = Content::default();
        lay_over(&mut c2, &files, &bad);
        assert!(c2.problems.iter().any(|p| p.starts_with("kept in this browser: maps/meadow.json")), "{:?}", c2.problems);
    }

    /// The zip read back as any tool reads one: each file's header, its bytes and CRC, then the
    /// list at the end pointing at them; a removed file left out.
    #[test]
    fn the_kept_files_as_one_zip() {
        let mut kept = BTreeMap::new();
        kept.insert("content/data/maps/meadow.json".to_string(), file("{ \"id\": \"meadow\" }\n"));
        kept.insert("content/assets/models/rock.glb".to_string(), Kept::File(Arc::new(vec![0, 1, 2, 255])));
        kept.insert("content/data/templates/hut.json".to_string(), Kept::Removed);
        let zip = zip(&kept);
        let u16_at = |i: usize| u16::from_le_bytes([zip[i], zip[i + 1]]);
        let u32_at = |i: usize| u32::from_le_bytes([zip[i], zip[i + 1], zip[i + 2], zip[i + 3]]);
        let end = zip.len() - 22;
        assert_eq!(u32_at(end), 0x0605_4b50);
        assert_eq!((u16_at(end + 8), u16_at(end + 10)), (2, 2), "two files");
        let (len, mut at) = (u32_at(end + 12) as usize, u32_at(end + 16) as usize);
        assert_eq!(at + len, end);
        let mut names = vec![];
        for _ in 0..2 {
            assert_eq!(u32_at(at), 0x0201_4b50);
            let (crc, size, name_len, header) = (u32_at(at + 16), u32_at(at + 20) as usize, u16_at(at + 28) as usize, u32_at(at + 42) as usize);
            let name = String::from_utf8(zip[at + 46..at + 46 + name_len].to_vec()).unwrap();
            // its header: the same name, then the bytes as kept
            assert_eq!(u32_at(header), 0x0403_4b50);
            assert_eq!(u16_at(header + 8), 0, "stored");
            let data = header + 30 + u16_at(header + 26) as usize + u16_at(header + 28) as usize;
            let bytes = &zip[data..data + size];
            assert_eq!(crc32fast::hash(bytes), crc);
            assert!(matches!(&kept[&name], Kept::File(b) if b.as_slice() == bytes));
            names.push(name);
            at += 46 + name_len;
        }
        assert_eq!(names, ["content/assets/models/rock.glb", "content/data/maps/meadow.json"]);
    }
}
