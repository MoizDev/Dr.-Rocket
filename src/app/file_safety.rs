//! Disk identity, lossless text formats, and non-replacing file operations.
use sha2::{Digest, Sha256};
use std::{io, path::Path};

pub fn fingerprint(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[derive(Default, Clone)]
pub struct TextFormat {
    crlf: bool,
    bom: bool,
    /// Keep the original line terminators for unchanged lines, including mixed-EOL files.
    lines: Vec<(String, &'static str)>,
}
impl TextFormat {
    pub fn decode(bytes: &[u8]) -> (String, Self, bool) {
        let valid = std::str::from_utf8(bytes).ok();
        let text = String::from_utf8_lossy(bytes);
        let bom = text.starts_with('\u{feff}');
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        let binary = text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t' | '\x0c'));
        let lines: Vec<_> = text
            .split_inclusive('\n')
            .map(|s| {
                if let Some(s) = s.strip_suffix("\r\n") {
                    (s.to_owned(), "\r\n")
                } else if let Some(s) = s.strip_suffix('\n') {
                    (s.to_owned(), "\n")
                } else {
                    (s.to_owned(), "")
                }
            })
            .collect();
        let crlf =
            lines.iter().filter(|(_, e)| *e == "\r\n").count() > lines.iter().filter(|(_, e)| *e == "\n").count();
        let mixed = lines.iter().any(|(_, e)| *e == "\r\n") && lines.iter().any(|(_, e)| *e == "\n");
        let lines = if mixed { lines } else { Vec::new() };
        (
            text.replace("\r\n", "\n"),
            Self { crlf, bom, lines },
            valid.is_none() || binary,
        )
    }
    pub fn encode(&self, text: &str) -> Vec<u8> {
        use std::collections::{HashMap, VecDeque};
        let mut endings: HashMap<&str, VecDeque<&str>> = HashMap::new();
        for (line, end) in &self.lines {
            if !end.is_empty() {
                endings.entry(line).or_default().push_back(end);
            }
        }
        let mut out = String::new();
        if self.bom {
            out.push('\u{feff}');
        }
        for (line_index, line) in text.split_inclusive('\n').enumerate() {
            if let Some(body) = line.strip_suffix('\n') {
                out.push_str(body);
                out.push_str(endings.get_mut(body).and_then(|v| v.pop_front()).unwrap_or_else(|| {
                    self.lines
                        .get(line_index)
                        .map(|(_, e)| *e)
                        .filter(|e| !e.is_empty())
                        .unwrap_or(if self.crlf { "\r\n" } else { "\n" })
                }));
            } else {
                out.push_str(line);
            }
        }
        out.into_bytes()
    }
}

/// Rename without replacing any directory entry, including dangling symlinks.
/// The kernel decides whether the destination is free; an existence check is not the guard.
pub fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    if from.as_os_str().as_encoded_bytes().contains(&0) || to.as_os_str().as_encoded_bytes().contains(&0) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Path contains NUL"));
    }
    if from == to {
        return Ok(());
    }
    // A case-insensitive filesystem may resolve both spellings to the same entry. A second
    // *actual* entry (a hardlink, symlink, or case-sensitive sibling) must still be rejected.
    let case_only = from.parent() == to.parent()
        && same_entry(from, to)
        && std::fs::symlink_metadata(to).is_ok()
        && !std::fs::read_dir(to.parent().unwrap_or(Path::new(".")))?
            .flatten()
            .any(|e| Some(e.file_name().as_os_str()) == to.file_name());
    if !case_only {
        return exclusive_rename(from, to);
    }
    let parent = from.parent().unwrap_or(Path::new("."));
    for n in 0..1000 {
        let temporary = parent.join(format!(".drrocket-rename-{}-{n}", std::process::id()));
        match exclusive_rename(from, &temporary) {
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
            Ok(()) => {
                if let Err(e) = exclusive_rename(&temporary, to) {
                    if let Err(rollback) = exclusive_rename(&temporary, from) {
                        return Err(io::Error::other(format!(
                            "{e}; original preserved at {} (rollback failed: {rollback})",
                            temporary.display()
                        )));
                    }
                    return Err(e);
                }
                return Ok(());
            }
        }
    }
    Err(io::Error::other("Could not allocate a temporary rename path"))
}

fn same_entry(from: &Path, to: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::symlink_metadata(from), std::fs::symlink_metadata(to)) {
            (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        match (from.canonicalize(), to.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
    }
}

pub fn fingerprint_path(path: &Path) -> io::Result<[u8; 32]> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("Not a regular file"));
    }
    let mut digest = Sha256::new();
    let mut chunk = [0; 65536];
    loop {
        let len = file.read(&mut chunk)?;
        if len == 0 {
            break;
        }
        digest.update(&chunk[..len]);
    }
    Ok(digest.finalize().into())
}

#[cfg(unix)]
fn exclusive_rename(from: &Path, to: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let a = CString::new(from.as_os_str().as_bytes())?;
    let b = CString::new(to.as_os_str().as_bytes())?;
    #[cfg(target_os = "macos")]
    unsafe extern "C" {
        fn renamex_np(a: *const std::ffi::c_char, b: *const std::ffi::c_char, flags: u32) -> i32;
    }
    #[cfg(target_os = "linux")]
    unsafe extern "C" {
        fn renameat2(a_fd: i32, a: *const std::ffi::c_char, b_fd: i32, b: *const std::ffi::c_char, flags: u32) -> i32;
    }
    // SAFETY: both pointers refer to live NUL-terminated path buffers for this call.
    #[cfg(target_os = "macos")]
    let result = unsafe { renamex_np(a.as_ptr(), b.as_ptr(), 4) }; // RENAME_EXCL
    #[cfg(target_os = "linux")]
    let result = unsafe { renameat2(-100, a.as_ptr(), -100, b.as_ptr(), 1) }; // AT_FDCWD, RENAME_NOREPLACE
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    return Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Exclusive rename is unsupported on this platform",
    ));
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
#[cfg(not(unix))]
fn exclusive_rename(from: &Path, to: &Path) -> io::Result<()> {
    // MoveFileExW without MOVEFILE_REPLACE_EXISTING (Rust's Windows rename).
    // Rust std currently uses REPLACE_EXISTING, so use MoveFileExW explicitly.
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn MoveFileExW(a: *const u16, b: *const u16, flags: u32) -> i32;
        }
        let a: Vec<_> = from.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<_> = to.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: the UTF-16 buffers are NUL-terminated and live for the duration of the call.
        if unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), 0) } != 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (from, to);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Exclusive rename unavailable",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lossless_line_endings_and_encoding() {
        for bytes in [
            b"#lang racket\r\n(a)\r\n".as_slice(),
            b"a\nb\r\nc",
            b"\xef\xbb\xbfa\r\n",
        ] {
            let (text, format, bad) = TextFormat::decode(bytes);
            assert!(!bad);
            assert_eq!(format.encode(&text), bytes);
        }
        assert!(TextFormat::decode(b"\xffdata").2);
        assert!(TextFormat::decode(b"binary\0data").2);
        let (_, f, _) = TextFormat::decode(b"a\r\nb\r\n");
        assert_eq!(f.encode("a\nnew\nb\n"), b"a\r\nnew\r\nb\r\n");
    }
    #[test]
    fn rename_preserves_existing_entries() {
        let dir = std::env::temp_dir().join(format!("drrocket-rename-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a"), dir.join("b"));
        std::fs::write(&a, b"source").unwrap();
        std::fs::write(&b, b"target").unwrap();
        assert!(rename_noreplace(&a, &b).is_err());
        assert_eq!(std::fs::read(&a).unwrap(), b"source");
        assert_eq!(std::fs::read(&b).unwrap(), b"target");
        std::fs::remove_file(&b).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.join("missing"), &b).unwrap();
            assert!(rename_noreplace(&a, &b).is_err());
            std::fs::remove_file(&b).unwrap();
        }
        rename_noreplace(&a, &b).unwrap();
        assert_eq!(std::fs::read(&b).unwrap(), b"source");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod rename_race_tests {
    use super::*;
    #[test]
    fn concurrent_rename_has_one_winner_and_preserves_loser() {
        let dir = std::env::temp_dir().join(format!("drrocket-rename-race-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a");
        let b = dir.join("b");
        let target = dir.join("target");
        std::fs::write(&a, b"A").unwrap();
        std::fs::write(&b, b"B").unwrap();
        let barrier = std::sync::Barrier::new(2);
        let (first, second) = std::thread::scope(|s| {
            let first = s.spawn(|| {
                barrier.wait();
                rename_noreplace(&a, &target)
            });
            let second = s.spawn(|| {
                barrier.wait();
                rename_noreplace(&b, &target)
            });
            (first.join().unwrap(), second.join().unwrap())
        });
        assert_ne!(first.is_ok(), second.is_ok());
        if first.is_ok() {
            assert_eq!(std::fs::read(&target).unwrap(), b"A");
            assert_eq!(std::fs::read(&b).unwrap(), b"B");
        } else {
            assert_eq!(std::fs::read(&target).unwrap(), b"B");
            assert_eq!(std::fs::read(&a).unwrap(), b"A");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn case_only_and_symlink_entry_renames_preserve_data() {
        let dir = std::env::temp_dir().join(format!("drrocket-case-rename-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let old = dir.join("Mixed.rkt");
        let new = dir.join("mixed.rkt");
        std::fs::write(&old, b"source").unwrap();
        rename_noreplace(&old, &new).unwrap();
        assert!(
            std::fs::read_dir(&dir)
                .unwrap()
                .flatten()
                .any(|e| e.file_name() == "mixed.rkt")
        );
        assert_eq!(std::fs::read(&new).unwrap(), b"source");
        #[cfg(unix)]
        {
            let alias = dir.join("alias");
            let moved = dir.join("moved-alias");
            std::os::unix::fs::symlink(&new, &alias).unwrap();
            rename_noreplace(&alias, &moved).unwrap();
            assert!(moved.symlink_metadata().unwrap().is_symlink());
            assert_eq!(std::fs::read(&new).unwrap(), b"source");
            let hardlink = dir.join("hardlink");
            std::fs::hard_link(&new, &hardlink).unwrap();
            assert!(rename_noreplace(&new, &hardlink).is_err());
            assert!(new.exists() && hardlink.exists());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
