use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use base64::Engine;
use once_cell::sync::Lazy;

const THUMB_SIZE: u32 = 200;

static CACHE: Lazy<Mutex<HashMap<(PathBuf, u64), String>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn thumb_dir() -> PathBuf {
    let mut d = std::env::temp_dir();
    d.push("doppelganger-thumbs");
    d
}

/// Generate (or return cached) base64 data URL for a 200px thumbnail of `path`.
/// macOS-only: shells to `qlmanage`. Returns Err if Quick Look fails to render.
pub fn thumbnail_data_url(path: &Path) -> Result<String, String> {
    let mtime = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map_err(|e| format!("stat: {e}"))?
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let key = (path.to_path_buf(), mtime);
    if let Some(cached) = CACHE.lock().unwrap().get(&key) {
        return Ok(cached.clone());
    }

    let dir = thumb_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir: {e}"))?;

    let status = Command::new("qlmanage")
        .args([
            "-t",
            "-s",
            &THUMB_SIZE.to_string(),
            "-o",
            dir.to_str().ok_or("dir is not utf8")?,
        ])
        .arg(path)
        .output()
        .map_err(|e| format!("qlmanage spawn: {e}"))?;

    if !status.status.success() {
        return Err(format!(
            "qlmanage failed: {}",
            String::from_utf8_lossy(&status.stderr)
        ));
    }

    let basename = path.file_name().ok_or("no filename")?;
    let mut produced = dir.join(basename);
    produced.as_mut_os_string().push(".png");

    let bytes = std::fs::read(&produced).map_err(|e| format!("read thumb: {e}"))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let data_url = format!("data:image/png;base64,{encoded}");

    CACHE.lock().unwrap().insert(key, data_url.clone());
    Ok(data_url)
}
