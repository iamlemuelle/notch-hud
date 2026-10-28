// Tests for CONTRACT.md section 12, "Video".
use super::*;
use std::fs;
use std::os::unix::fs::symlink;

/// A fresh empty directory under the temp dir, removed when dropped.
struct Dir(PathBuf);

impl Dir {
    fn new(test: &str) -> Dir {
        let path = std::env::temp_dir().join(format!("notch-hud-video-{}-{test}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Dir(path)
    }

    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap()
}

#[test]
fn a_good_mp4_is_returned_canonical_with_its_name() {
    let dir = Dir::new("good");
    let path = dir.file("clip.mp4", b"bytes");
    let video = parse_path(&path).unwrap();
    assert_eq!(video, Video { path: canonical(&path), name: "clip.mp4".into() });
    assert!(!video.path.starts_with("/var/"), "{:?} is not canonical", video.path);
}

#[test]
fn extensions_are_case_insensitive_and_m4v_and_mov_are_accepted() {
    for (i, name) in ["CLIP.MOV", "clip.m4v", "Clip.Mp4", "clip.mov", "CLIP.M4V"].into_iter().enumerate() {
        // One directory each: the temp volume may ignore case, so clip.mov and CLIP.MOV would be one file.
        let dir = Dir::new(&format!("ext{i}"));
        let path = dir.file(name, b"bytes");
        assert_eq!(parse_path(&path), Ok(Video { path: canonical(&path), name: name.into() }), "{name}");
    }
}

#[test]
fn a_directory_named_like_a_video_is_not_a_regular_file() {
    let dir = Dir::new("dir");
    let path = dir.0.join("dir.mp4");
    fs::create_dir(&path).unwrap();
    assert_eq!(parse_path(&path), Err("not a regular file".to_string()));
}

#[test]
fn another_extension_is_not_an_mp4() {
    let dir = Dir::new("txt");
    let path = dir.file("notes.txt", b"bytes");
    assert_eq!(parse_path(&path), Err("not an mp4, m4v or mov file".to_string()));
}

#[test]
fn no_extension_is_not_an_mp4() {
    let dir = Dir::new("noext");
    let path = dir.file("clip", b"bytes");
    assert_eq!(parse_path(&path), Err("not an mp4, m4v or mov file".to_string()));
}

#[test]
fn an_empty_file_is_rejected() {
    let dir = Dir::new("empty");
    let path = dir.file("empty.mp4", b"");
    assert_eq!(parse_path(&path), Err("empty file".to_string()));
}

#[test]
fn a_relative_path_is_rejected() {
    assert_eq!(parse_path(Path::new("clip.mp4")), Err("not an absolute path".to_string()));
    assert_eq!(parse_path(Path::new("./dir/clip.mp4")), Err("not an absolute path".to_string()));
}

#[test]
fn a_missing_file_cannot_be_resolved() {
    let dir = Dir::new("missing");
    let error = parse_path(&dir.0.join("gone.mp4")).unwrap_err();
    assert!(error.starts_with("cannot resolve: "), "{error}");
}

#[test]
fn a_symlink_to_a_video_returns_the_target() {
    let dir = Dir::new("link");
    let real = dir.file("real.mp4", b"bytes");
    let link = dir.0.join("link.mp4");
    symlink(&real, &link).unwrap();
    assert_eq!(parse_path(&link), Ok(Video { path: canonical(&real), name: "real.mp4".into() }));
}

#[test]
fn a_symlink_named_mp4_to_a_text_file_is_rejected() {
    let dir = Dir::new("fake");
    let notes = dir.file("notes.txt", b"bytes");
    let fake = dir.0.join("fake.mp4");
    symlink(&notes, &fake).unwrap();
    assert_eq!(parse_path(&fake), Err("not an mp4, m4v or mov file".to_string()));
}

#[test]
fn dot_dot_segments_resolve() {
    let dir = Dir::new("dots");
    let path = dir.file("clip.mp4", b"bytes");
    fs::create_dir(dir.0.join("sub")).unwrap();
    let indirect = dir.0.join("sub").join("..").join("clip.mp4");
    assert_eq!(parse_path(&indirect), Ok(Video { path: canonical(&path), name: "clip.mp4".into() }));
}

#[test]
fn a_video_serializes_to_path_and_name() {
    let video = Video { path: PathBuf::from("/Users/lem/clip.mp4"), name: "clip.mp4".into() };
    assert_eq!(serde_json::to_string(&video).unwrap(), r#"{"path":"/Users/lem/clip.mp4","name":"clip.mp4"}"#);
}
