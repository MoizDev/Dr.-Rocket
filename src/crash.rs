//! Crash reports for testers: a panic writes a report (version, OS, the error and a
//! backtrace) to the config folder; the next launch says so, and Report a Problem copies it.

use std::path::PathBuf;

fn dir() -> PathBuf {
    crate::config::config_dir().join("crash-reports")
}

/// Version, platform and Racket, for reports.
pub fn about(racket: Option<&str>) -> String {
    format!(
        "Dr. Rocket {} on {} {} (Racket {})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        racket.unwrap_or("not found")
    )
}

/// Install the panic hook (keeps the default one, which prints to stderr).
pub fn install() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let when = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        let report = format!(
            "{}\nTime: {when} (Unix)\nThread: {thread}\n\n{info}\n\nBacktrace:\n{}\n",
            about(None),
            std::backtrace::Backtrace::force_capture()
        );
        let _ = std::fs::create_dir_all(dir());
        let _ = std::fs::write(dir().join(format!("crash-{when}.txt")), report);
        default(info);
    }));
}

/// Reports written since the user last saw one (oldest first).
pub fn unseen() -> Vec<PathBuf> {
    let seen = std::fs::metadata(dir().join(".seen")).and_then(|m| m.modified()).ok();
    let mut out: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("crash-"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .filter(|(t, _)| seen.is_none_or(|s| *t > s))
        .collect();
    out.sort();
    out.into_iter().map(|(_, p)| p).collect()
}

pub fn mark_seen() {
    let _ = std::fs::create_dir_all(dir());
    let _ = std::fs::write(dir().join(".seen"), b"");
}

/// The text Report a Problem copies: the environment and the latest crash report, if any.
pub fn problem_report(racket: Option<&str>) -> String {
    let latest = std::fs::read_dir(dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("crash-"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .max();
    let mut out = format!("{}\n\nWhat happened (please describe):\n\n\n", about(racket));
    if let Some((_, p)) = latest {
        out.push_str(&format!("Latest crash report ({}):\n", p.display()));
        out.push_str(&std::fs::read_to_string(&p).unwrap_or_default());
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn panics_leave_a_report_that_report_a_problem_copies() {
        let dir = std::env::temp_dir().join(format!("drrocket-crash-test-{}", std::process::id()));
        // SAFETY: tests touching the config dir run single-threaded with respect to it.
        unsafe { std::env::set_var("DRROCKET_CONFIG_DIR", &dir) };
        super::install();
        let _ = std::thread::Builder::new().name("worker".into()).spawn(|| panic!("boom in a test")).unwrap().join();
        let unseen = super::unseen();
        assert_eq!(unseen.len(), 1);
        let report = super::problem_report(Some("9.3"));
        assert!(report.contains("Racket 9.3") && report.contains("boom in a test") && report.contains("Thread: worker"), "{report}");
        super::mark_seen();
        assert!(super::unseen().is_empty());
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::panic::take_hook();
    }
}
