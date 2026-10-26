//! The file dropped on the notch for the video stage (CONTRACT.md section 12). No app state: the caller keeps the
//! result.

use std::path::{Path, PathBuf};

/// The extensions the stage accepts, compared without regard to case.
pub const EXTENSIONS: [&str; 3] = ["mp4", "m4v", "mov"];

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Video {
    /// Canonical: absolute, with every symlink resolved, so it names the file that will be read.
    pub path: PathBuf,
    /// The canonical path's file name.
    pub name: String,
}

/// A dropped path as a playable file: absolute, canonicalized, a regular file with an accepted extension and at least
/// one byte. A symlink is judged by its target.
pub fn parse_path(path: &Path) -> Result<Video, String> {
    if !path.is_absolute() {
        return Err("not an absolute path".into());
    }
    let path = std::fs::canonicalize(path).map_err(|e| format!("cannot resolve: {e}"))?;
    let meta = std::fs::metadata(&path).map_err(|e| format!("cannot read: {e}"))?;
    if !meta.is_file() {
        return Err("not a regular file".into());
    }
    let extension = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    if !extension.is_some_and(|e| EXTENSIONS.contains(&e.as_str())) {
        return Err("not an mp4, m4v or mov file".into());
    }
    if meta.len() == 0 {
        return Err("empty file".into());
    }
    // The page gets the path as a JSON string, which a path that is not UTF-8 can't be.
    if path.to_str().is_none() {
        return Err("path is not UTF-8".into());
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
    Ok(Video { path, name })
}

#[cfg(test)]
#[path = "video_tests.rs"]
mod tests;
