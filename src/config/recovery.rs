//! Recover valid siblings without replacing malformed originals or losing unknown fields.
use super::atomic_write;
#[cfg(test)]
use super::{CursorShape, Settings, atomic_write_checked};
use serde::Serialize;
use std::path::{Path, PathBuf};

static RECOVERY_NOTICES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
pub fn take_recovery_notices() -> Vec<String> {
    std::mem::take(&mut *RECOVERY_NOTICES.lock().unwrap())
}

fn json_value(bytes: &[u8]) -> Result<serde_json::Value, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    serde_json::from_str(&crate::theme::strip_json_comments(text.trim_start_matches('\u{feff}')))
        .map_err(|e| e.to_string())
}
fn backup_bad(path: &Path, bytes: &[u8]) -> std::io::Result<PathBuf> {
    use std::io::Write;
    for n in 0..10_000 {
        let backup = path.with_extension(format!("json.recovery-{n}.bak"));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&backup) {
            Ok(mut f) => {
                f.write_all(bytes)?;
                f.sync_all()?;
                return Ok(backup);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if std::fs::read(&backup).is_ok_and(|old| old == bytes) {
                    return Ok(backup);
                }
            }
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("Too many recovery backups"))
}

/// Validate additions against the complete type, retaining independently valid siblings.
/// Unknown fields survive in the source file and are merged back on save.
fn recover_value<T: serde::de::DeserializeOwned + Serialize + Default>(raw: &serde_json::Value) -> T {
    use serde_json::Value;
    fn overlay<T: serde::de::DeserializeOwned>(root: &mut Value, path: &str, value: &Value) {
        let old = root.clone();
        if path.is_empty() {
            *root = value.clone();
        } else if let Some(slot) = root.pointer_mut(path) {
            *slot = value.clone();
        }
        if serde_json::from_value::<T>(root.clone()).is_ok() {
            return;
        }
        *root = old;
        match value {
            Value::Object(map) => {
                // Optional/defaultable objects need an object container before leaf recovery.
                if !root.pointer(path).is_some_and(Value::is_object) {
                    let mut candidate = root.clone();
                    if let Some(slot) = candidate.pointer_mut(path) {
                        *slot = serde_json::json!({});
                    }
                    if serde_json::from_value::<T>(candidate.clone()).is_ok() {
                        *root = candidate;
                    }
                }
                for (key, val) in map {
                    let child = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                    if root.pointer(&child).is_none() {
                        for seed in [
                            val.clone(),
                            serde_json::json!({}),
                            serde_json::json!([]),
                            serde_json::Value::Null,
                        ] {
                            let mut candidate = root.clone();
                            if let Some(Value::Object(object)) = candidate.pointer_mut(path) {
                                object.insert(key.clone(), seed);
                            }
                            if serde_json::from_value::<T>(candidate.clone()).is_ok()
                                && candidate.pointer(&child).is_some()
                            {
                                *root = candidate;
                                break;
                            }
                        }
                    }
                    if root.pointer(&child).is_some() {
                        overlay::<T>(root, &child, val);
                    }
                }
            }
            Value::Array(items) => {
                if let Some(slot) = root.pointer_mut(path) {
                    *slot = Value::Array(Vec::new());
                }
                for item in items {
                    let mut candidate = root.clone();
                    if let Some(Value::Array(array)) = candidate.pointer_mut(path) {
                        array.push(item.clone());
                    }
                    if serde_json::from_value::<T>(candidate.clone()).is_ok() {
                        *root = candidate;
                    } else if item.is_object() {
                        // Defaultable records (e.g. projects) can themselves be recovered.
                        let mut candidate = root.clone();
                        let mut child = None;
                        if let Some(Value::Array(array)) = candidate.pointer_mut(path) {
                            child = Some(format!("{path}/{}", array.len()));
                            array.push(serde_json::json!({}));
                        }
                        if serde_json::from_value::<T>(candidate.clone()).is_ok() {
                            *root = candidate;
                            if let Some(child) = child {
                                overlay::<T>(root, &child, item);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let mut value = serde_json::to_value(T::default()).expect("default serializes");
    overlay::<T>(&mut value, "", raw);
    serde_json::from_value(value).unwrap_or_default()
}

pub fn load_recovering<T: serde::de::DeserializeOwned + Serialize + Default>(path: &Path) -> T {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                RECOVERY_NOTICES
                    .lock()
                    .unwrap()
                    .push(format!("Could not read {}: {e}", path.display()));
            }
            return T::default();
        }
    };
    let raw = json_value(&bytes);
    if let Ok(value) = &raw
        && let Ok(data) = serde_json::from_value::<T>(value.clone())
    {
        return data;
    }
    let message = match backup_bad(path, &bytes) {
        Ok(backup) => format!(
            "Recovered valid fields from {}. Original data is preserved at {}.",
            path.display(),
            backup.display()
        ),
        Err(e) => format!(
            "Could not back up {}: {e}. Replacement is blocked until the file can be backed up.",
            path.display()
        ),
    };
    RECOVERY_NOTICES.lock().unwrap().push(message);
    raw.as_ref().map(recover_value::<T>).unwrap_or_default()
}

/// Back up malformed data before replacement; retain fields written by newer versions.
pub fn write_preserving_unknown<T: serde::de::DeserializeOwned>(path: &Path, text: &str) -> std::io::Result<()> {
    fn merge(old: &mut serde_json::Value, new: serde_json::Value) {
        match (old, new) {
            (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
                for (k, v) in b {
                    if let Some(old) = a.get_mut(&k) {
                        merge(old, v);
                    } else {
                        a.insert(k, v);
                    }
                }
            }
            (a, b) => *a = b,
        }
    }
    let new: serde_json::Value = serde_json::from_str(text)?;
    let mut output = new.clone();
    match std::fs::read(path) {
        Ok(bytes) => {
            let raw = json_value(&bytes);
            if raw
                .as_ref()
                .ok()
                .and_then(|v| serde_json::from_value::<T>(v.clone()).ok())
                .is_none()
            {
                backup_bad(path, &bytes)?;
            }
            match raw {
                Ok(mut raw) => {
                    merge(&mut raw, new);
                    output = raw;
                }
                Err(e) => {
                    return Err(std::io::Error::other(format!(
                        "Malformed JSON is backed up; fix {} before saving: {e}",
                        path.display()
                    )));
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    atomic_write(path, serde_json::to_string_pretty(&output)?.as_bytes())
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    #[test]
    fn malformed_fields_do_not_discard_valid_settings() {
        let raw = json_value(b"\xef\xbb\xbf{\"theme\":\"chosen\",\"buffer_font_size\":18,\"cursor_shape\":\"future\",\"autosave\":\"oops\"}").unwrap();
        let recovered: Settings = recover_value(&raw);
        assert_eq!(recovered.theme, "chosen");
        assert_eq!(recovered.buffer_font_size, 18.0);
        assert_eq!(recovered.cursor_shape, CursorShape::Bar);
    }
    #[test]
    fn session_unknown_panel_retains_projects_and_layout() {
        let raw = serde_json::json!({"projects":[{"project":"/scratch","files":["a.rkt"]}],"layout":{"left":{"open":true,"size":300.0,"active":0,"items":["Files","FuturePanel","Search"]}}});
        let s: crate::app::Session = recover_value(&raw);
        assert_eq!(s.projects.len(), 1);
        assert_eq!(s.layout.left.size, 300.0);
        assert_eq!(
            s.layout.left.items,
            vec![crate::app::PanelItem::Files, crate::app::PanelItem::Search]
        );
    }
}

#[cfg(test)]
mod disk_recovery_tests {
    use super::*;
    #[test]
    fn backups_precede_replacement_and_unknown_fields_survive() {
        let dir = std::env::temp_dir().join(format!("drrocket-json-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let original = br##"{"theme":"retained","cursor_shape":"future","future_setting":{"enabled":true}}"##;
        std::fs::write(&path, original).unwrap();
        let mut settings: Settings = load_recovering(&path);
        assert_eq!(settings.theme, "retained");
        assert_eq!(
            std::fs::read(path.with_extension("json.recovery-0.bak")).unwrap(),
            original
        );
        settings.autosave = true;
        write_preserving_unknown::<Settings>(&path, &serde_json::to_string(&settings).unwrap()).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["future_setting"]["enabled"], true);
        assert_eq!(value["theme"], "retained");
        std::fs::write(&path, b"{broken syntax").unwrap();
        assert!(write_preserving_unknown::<Settings>(&path, &serde_json::to_string(&settings).unwrap()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{broken syntax");
        assert_eq!(
            std::fs::read(path.with_extension("json.recovery-1.bak")).unwrap(),
            b"{broken syntax"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn nested_malformed_settings_and_project_members_recover_independently() {
        let raw = serde_json::json!({"theme":"mine","default_language_settings":{"teachpacks":["keep"],"printing_style":"future","case_sensitive":false}});
        let settings: Settings = recover_value(&raw);
        let teaching = settings.default_language_settings.unwrap();
        assert_eq!(teaching.teachpacks, vec!["keep"]);
        assert!(!teaching.case_sensitive);
        let raw = serde_json::json!({"projects":[{"project":"/scratch","files":["keep.rkt"],"active":"broken"},{"project":"/second"}]});
        let session: crate::app::Session = recover_value(&raw);
        assert_eq!(session.projects.len(), 2);
        assert_eq!(session.projects[0].files, vec![PathBuf::from("keep.rkt")]);
    }
    #[test]
    fn final_save_guard_preserves_concurrent_external_edit_and_cleans_temporary() {
        let dir = std::env::temp_dir().join(format!("drrocket-guard-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("source.rkt");
        std::fs::write(&path, b"original").unwrap();
        let result = atomic_write_checked(&path, b"local edits", || {
            std::fs::write(&path, b"external edits")?;
            Err(std::io::Error::other("disk changed"))
        });
        assert!(result.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"external edits");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
