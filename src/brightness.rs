//! Optional brightness must never prevent the panel from displaying readings.
//!
//! Arm a per-user marker before entering the HID driver and remove it only on
//! success. An error, panic or watchdog exit leaves it behind, so reconnects
//! and scheduled restarts cannot repeatedly enter a broken brightness call.

use std::fmt::Display;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;

pub const MARKER: &str = "wireview-nexus-brightness.guard";

pub fn guard_path() -> Option<PathBuf> {
    let var = |key| std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = if cfg!(windows) {
        var("LOCALAPPDATA")
    } else {
        var("XDG_STATE_HOME").or_else(|| var("HOME").map(|p| p.join(".local/state")))
    };
    base.map(|p| p.join("wireview").join(MARKER))
}

pub fn reset() -> io::Result<()> {
    let path = guard_path().ok_or_else(|| io::Error::other("no per-user state directory available"))?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Applied,
    Skipped,
    /// Emitted once per process. The caller should log it and keep rendering.
    Disabled(String),
}

pub struct BrightnessGuard {
    path: Option<PathBuf>,
    disabled: bool,
}

impl Default for BrightnessGuard {
    fn default() -> Self {
        Self {
            path: guard_path(),
            disabled: false,
        }
    }
}

impl BrightnessGuard {
    pub fn apply<E: Display>(&mut self, pct: u8, send: impl FnOnce() -> Result<(), E>) -> Outcome {
        if self.disabled {
            return Outcome::Skipped;
        }
        // Latch before any fallible work, including the driver call. Only a
        // completely successful attempt re-enables brightness on reconnect.
        self.disabled = true;
        let Some(path) = &self.path else {
            return Outcome::Disabled("cannot protect brightness without a per-user state directory".into());
        };
        let arm = || -> io::Result<()> {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(path)?;
            writeln!(
                file,
                "Brightness {pct}% did not complete successfully. Stop the daemon, then run --reset-brightness-guard to retry."
            )?;
            file.sync_all()
        };
        if let Err(e) = arm() {
            let why = if e.kind() == io::ErrorKind::AlreadyExists {
                "an earlier brightness attempt failed or was interrupted".to_string()
            } else {
                format!("cannot arm brightness guard: {e}")
            };
            return Outcome::Disabled(format!("{why} ({})", path.display()));
        }
        if let Err(e) = send() {
            return Outcome::Disabled(format!("brightness report failed: {e} ({})", path.display()));
        }
        if let Err(e) = fs::remove_file(path) {
            return Outcome::Disabled(format!("brightness was applied, but its guard could not be cleared: {e}"));
        }
        self.disabled = false;
        Outcome::Applied
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn guard() -> BrightnessGuard {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "wireview-brightness-{}-{}.guard",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_file(&path);
        BrightnessGuard {
            path: Some(path),
            disabled: false,
        }
    }

    #[test]
    fn failures_do_not_repeat_on_reconnect_or_restart() {
        let mut guard = guard();
        let mut attempts = 0;
        let mut warnings = 0;
        for _ in 0..100 {
            let outcome = guard.apply(50, || {
                attempts += 1;
                Err("unsupported report")
            });
            warnings += usize::from(matches!(outcome, Outcome::Disabled(_)));
            assert!(matches!(outcome, Outcome::Disabled(_) | Outcome::Skipped));
            // The caller proceeds to frame delivery for all outcomes.
        }
        assert_eq!(attempts, 1);
        assert_eq!(warnings, 1);
        let mut restarted = BrightnessGuard {
            path: guard.path.clone(),
            disabled: false,
        };
        assert!(matches!(
            restarted.apply(80, || {
                attempts += 1;
                Ok::<_, &str>(())
            }),
            Outcome::Disabled(_)
        ));
        assert_eq!(attempts, 1, "a new process or brightness value must not bypass the guard");
        fs::remove_file(guard.path.unwrap()).unwrap();
    }

    #[test]
    fn successful_reports_can_be_applied_again_on_reconnect() {
        let mut guard = guard();
        for _ in 0..3 {
            let path = guard.path.clone().unwrap();
            assert_eq!(
                guard.apply(50, || {
                    assert!(path.is_file(), "guard must be durable before entering the driver");
                    Ok::<_, &str>(())
                }),
                Outcome::Applied
            );
            assert!(!path.exists());
        }
    }

    #[test]
    fn an_interrupted_attempt_leaves_the_next_process_protected() {
        const CHILD: &str = "WIREVIEW_BRIGHTNESS_EXIT_TEST";
        if let Some(path) = std::env::var_os(CHILD) {
            let mut guard = BrightnessGuard {
                path: Some(path.into()),
                disabled: false,
            };
            // Same abrupt exit as the watchdog: no destructors or unwinding.
            guard.apply::<&str>(50, || std::process::exit(3));
            panic!("the child never entered the guarded operation");
        }
        let guard = guard();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "brightness::tests::an_interrupted_attempt_leaves_the_next_process_protected",
                "--nocapture",
            ])
            .env(CHILD, guard.path.as_ref().unwrap())
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(3), "{}", String::from_utf8_lossy(&result.stderr));
        let mut restarted = BrightnessGuard {
            path: guard.path.clone(),
            disabled: false,
        };
        assert!(matches!(
            restarted.apply::<&str>(50, || panic!("must not retry")),
            Outcome::Disabled(_)
        ));
        // Explicit reset permits a new process to try again.
        fs::remove_file(guard.path.as_ref().unwrap()).unwrap();
        let mut after_reset = BrightnessGuard {
            path: guard.path,
            disabled: false,
        };
        assert_eq!(after_reset.apply(50, || Ok::<_, &str>(())), Outcome::Applied);
    }

    #[test]
    fn unavailable_guard_storage_never_enters_the_driver() {
        let mut guard = guard();
        let file = guard.path.clone().unwrap();
        fs::write(&file, "not a directory").unwrap();
        guard.path = Some(file.join("child"));
        assert!(matches!(
            guard.apply::<&str>(50, || panic!("must not enter driver unguarded")),
            Outcome::Disabled(_)
        ));
        assert_eq!(guard.apply::<&str>(50, || panic!("must stay disabled")), Outcome::Skipped);
        fs::remove_file(file).unwrap();
    }
}
