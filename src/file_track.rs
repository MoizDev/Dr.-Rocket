//! Follow open files when they, or a folder above them, are moved or renamed outside the app
//! (in Finder, say), so saving writes to where the file is now instead of failing.
//!
//! On macOS this uses a bookmark, as other Mac apps do. Elsewhere files aren't followed, and
//! saving after a move asks where to save instead.

use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct FileRef {
    /// The file's (device, inode) when the bookmark was made. Saving replaces the file, so a
    /// new id means the bookmark needs remaking.
    #[cfg(target_os = "macos")]
    id: Option<(u64, u64)>,
    #[cfg(target_os = "macos")]
    bookmark: Option<Vec<u8>>,
}

impl FileRef {
    pub fn new(path: Option<&Path>) -> Self {
        #[cfg(target_os = "macos")]
        if let Some(path) = path {
            return FileRef { id: std::fs::metadata(path).ok().map(|m| file_id(&m)), bookmark: mac::bookmark(path) };
        }
        let _ = path;
        FileRef::default()
    }

    /// Where the file is now, when it is no longer at `path` because it (or a folder above it)
    /// was moved or renamed. None when it's still there, or when it can't be found: deleted,
    /// in the Trash, or on a drive that isn't connected.
    pub fn moved(&mut self, path: &Path) -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            match std::fs::metadata(path) {
                Ok(m) => {
                    if self.id != Some(file_id(&m)) {
                        *self = FileRef::new(Some(path));
                    }
                    None
                }
                Err(_) => {
                    let now = mac::resolve(self.bookmark.as_deref()?)?;
                    if now == path || in_trash(&now) || !now.is_file() {
                        return None;
                    }
                    *self = FileRef::new(Some(&now));
                    Some(now)
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = path;
            None
        }
    }
}

#[cfg(target_os = "macos")]
fn file_id(m: &std::fs::Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (m.dev(), m.ino())
}

/// A file moved to the Trash is deleted as far as the editor is concerned.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn in_trash(path: &Path) -> bool {
    path.components().any(|c| matches!(c.as_os_str().to_str(), Some(".Trash" | ".Trashes")))
}

#[cfg(target_os = "macos")]
mod mac {
    use objc2_foundation::{NSData, NSString, NSURL, NSURLBookmarkCreationOptions, NSURLBookmarkResolutionOptions};
    use std::path::{Path, PathBuf};

    pub fn bookmark(path: &Path) -> Option<Vec<u8>> {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_str()?));
        let data = url.bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(NSURLBookmarkCreationOptions::MinimalBookmark, None, None).ok()?;
        Some(data.to_vec())
    }

    pub fn resolve(bookmark: &[u8]) -> Option<PathBuf> {
        let data = NSData::with_bytes(bookmark);
        let mut stale = objc2::runtime::Bool::NO;
        // Never show UI or mount a drive just to look for a file.
        let options = NSURLBookmarkResolutionOptions::WithoutUI | NSURLBookmarkResolutionOptions::WithoutMounting;
        // SAFETY: `stale` is a valid pointer for the duration of the call.
        let url = unsafe { NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(&data, options, None, &mut stale) }.ok()?;
        Some(PathBuf::from(url.path()?.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trash_is_not_a_new_home() {
        assert!(in_trash(Path::new("/Users/a/.Trash/CS135/a1.rkt")));
        assert!(in_trash(Path::new("/Volumes/USB/.Trashes/501/a1.rkt")));
        assert!(!in_trash(Path::new("/Users/a/Documents/CS135/a1.rkt")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn follows_a_moved_folder_and_a_save_after_it() {
        let root = std::env::temp_dir().join(format!("drrocket-track-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("CS135")).unwrap();
        std::fs::create_dir_all(root.join("Moved")).unwrap();
        let root = root.canonicalize().unwrap();
        let path = root.join("CS135/a1.rkt");
        std::fs::write(&path, "1").unwrap();
        let mut file = FileRef::new(Some(&path));
        assert_eq!(file.moved(&path), None);

        // Saving replaces the file; the reference keeps up with it.
        crate::config::atomic_write(&path, b"2").unwrap();
        assert_eq!(file.moved(&path), None);

        std::fs::rename(root.join("CS135"), root.join("Moved/CS135")).unwrap();
        let now = root.join("Moved/CS135/a1.rkt");
        assert_eq!(file.moved(&path), Some(now.clone()));
        assert_eq!(file.moved(&now), None);

        // Renamed in place, then deleted.
        let renamed = root.join("Moved/CS135/a1-final.rkt");
        std::fs::rename(&now, &renamed).unwrap();
        assert_eq!(file.moved(&now), Some(renamed.clone()));
        std::fs::remove_file(&renamed).unwrap();
        assert_eq!(file.moved(&renamed), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
