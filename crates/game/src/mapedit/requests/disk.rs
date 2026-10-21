//! The requests' folder (private/requests/): the project's own on the desktop; in the
//! browser, the files this browser keeps of the editor's (`crate::keep`, at the same paths), so
//! the window works the same on both. The calls are std::fs's, as the window makes them.

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A folder's entry: its path, its name, and whether it's a folder itself.
pub(in super::super) struct Entry {
    pub path: PathBuf,
    pub name: String,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub dir: bool,
}

#[cfg(not(target_arch = "wasm32"))]
mod here {
    use super::*;

    pub use std::fs::{create_dir_all, read, read_to_string, remove_dir_all, remove_file, rename, write};

    pub fn read_dir(dir: &Path) -> io::Result<Vec<Entry>> {
        Ok(std::fs::read_dir(dir)?
            .flatten()
            .map(|e| Entry { path: e.path(), name: e.file_name().to_string_lossy().to_string(), dir: e.file_type().map(|t| t.is_dir()).unwrap_or(false) })
            .collect())
    }

    pub fn is_file(path: &Path) -> bool {
        path.is_file()
    }

    pub fn is_dir(path: &Path) -> bool {
        path.is_dir()
    }

    pub fn modified(path: &Path) -> Option<SystemTime> {
        std::fs::metadata(path).and_then(|m| m.modified()).ok()
    }

    pub fn now() -> SystemTime {
        SystemTime::now()
    }
}

#[cfg(target_arch = "wasm32")]
mod here {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::time::Duration;

    use super::*;
    use crate::keep;

    thread_local! {
        /// when each file was last written here (ms); those kept from before, when the page
        /// first asked
        static WRITTEN: RefCell<HashMap<String, f64>> = RefCell::new(HashMap::new());
    }

    /// A path as the browser keeps it: the project's, with forward slashes.
    fn key(path: &Path) -> String {
        let s = path.to_string_lossy().replace('\\', "/");
        let s = s.trim_start_matches("./").trim_start_matches('/');
        s.trim_end_matches('/').to_string()
    }

    fn missing(path: &Path) -> io::Error {
        io::Error::new(io::ErrorKind::NotFound, format!("{} isn't kept in this browser", path.display()))
    }

    fn stamp(k: &str, written: bool) -> f64 {
        WRITTEN.with(|w| {
            let mut w = w.borrow_mut();
            let now = js_sys::Date::now();
            if written {
                // later than before, however fast it's written again
                let at = w.get(k).map(|t| (t + 1.0).max(now)).unwrap_or(now);
                w.insert(k.to_string(), at);
                at
            } else {
                *w.entry(k.to_string()).or_insert(now)
            }
        })
    }

    pub fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
        let path = path.as_ref();
        keep::get(&key(path)).map(|b| b.to_vec()).ok_or_else(|| missing(path))
    }

    pub fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
        String::from_utf8(read(path)?).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> io::Result<()> {
        let k = key(path.as_ref());
        keep::keep(&k, bytes.as_ref().to_vec());
        stamp(&k, true);
        Ok(())
    }

    /// (folders are the paths under them: nothing to make)
    pub fn create_dir_all(_path: impl AsRef<Path>) -> io::Result<()> {
        Ok(())
    }

    pub fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        let k = key(path);
        if keep::get(&k).is_none() {
            return Err(missing(path));
        }
        keep::forget(&k);
        WRITTEN.with(|w| w.borrow_mut().remove(&k));
        Ok(())
    }

    pub fn remove_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        let under = keep::under(&format!("{}/", key(path)));
        if under.is_empty() {
            return Err(missing(path));
        }
        for k in under {
            keep::forget(&k);
            WRITTEN.with(|w| w.borrow_mut().remove(&k));
        }
        Ok(())
    }

    /// A file, or a folder with everything in it, moved.
    pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
        let (from, to) = (from.as_ref(), to.as_ref());
        let (f, t) = (key(from), key(to));
        let mut moved: Vec<(String, String)> = keep::under(&format!("{f}/")).into_iter().map(|k| (k.clone(), format!("{t}{}", &k[f.len()..]))).collect();
        if keep::get(&f).is_some() {
            moved.push((f.clone(), t.clone()));
        }
        if moved.is_empty() {
            return Err(missing(from));
        }
        for (a, b) in moved {
            if let Some(bytes) = keep::get(&a) {
                keep::keep(&b, bytes.to_vec());
                keep::forget(&a);
                WRITTEN.with(|w| w.borrow_mut().remove(&a));
                stamp(&b, true);
            }
        }
        Ok(())
    }

    pub fn read_dir(dir: &Path) -> io::Result<Vec<Entry>> {
        let base = key(dir);
        let prefix = if base.is_empty() { String::new() } else { format!("{base}/") };
        let mut out: Vec<Entry> = vec![];
        for k in keep::under(&prefix) {
            let rest = &k[prefix.len()..];
            let (name, deeper) = match rest.split_once('/') {
                Some((n, _)) => (n, true),
                None => (rest, false),
            };
            if !out.iter().any(|e| e.name == name) {
                out.push(Entry { path: dir.join(name), name: name.to_string(), dir: deeper });
            }
        }
        if out.is_empty() {
            return Err(missing(dir));
        }
        Ok(out)
    }

    pub fn is_file(path: &Path) -> bool {
        keep::get(&key(path)).is_some()
    }

    pub fn is_dir(path: &Path) -> bool {
        !keep::under(&format!("{}/", key(path))).is_empty()
    }

    pub fn modified(path: &Path) -> Option<SystemTime> {
        let k = key(path);
        keep::get(&k)?;
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs_f64(stamp(&k, false) / 1000.0))
    }

    pub fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs_f64(js_sys::Date::now() / 1000.0)
    }
}

pub(in super::super) use here::*;

/// Whether there's a file or a folder there.
pub(in super::super) fn exists(path: &Path) -> bool {
    is_file(path) || is_dir(path)
}
