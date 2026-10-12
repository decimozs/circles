//! Keeps the Mac awake while a run that nobody is watching is going. It runs `caffeinate`, which
//! lets go as soon as it is stopped or Circles ends.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

pub struct KeepAwake {
    program: PathBuf,
    args: Vec<String>,
    child: Mutex<Option<Child>>,
}

impl Default for KeepAwake {
    fn default() -> Self {
        // Idle sleep is prevented, and `-w` ends it when this process ends, whatever the reason.
        Self::with(
            "/usr/bin/caffeinate",
            vec!["-i".into(), "-w".into(), std::process::id().to_string()],
        )
    }
}

impl KeepAwake {
    pub fn with(program: impl Into<PathBuf>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            child: Mutex::new(None),
        }
    }

    pub fn is_on(&self) -> bool {
        let mut guard = self.child.lock().expect("awake poisoned");
        match guard.as_mut().map(Child::try_wait) {
            Some(Ok(None)) => true,
            Some(_) => {
                *guard = None;
                false
            }
            None => false,
        }
    }

    /// Starts or stops keeping the Mac awake. Does nothing when it already is as wanted.
    pub fn set(&self, on: bool) {
        if on == self.is_on() {
            return;
        }
        let mut guard = self.child.lock().expect("awake poisoned");
        if on {
            match Command::new(&self.program)
                .args(&self.args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(child) => *guard = Some(child),
                Err(e) => tracing::warn!(error = %e, "cannot keep the Mac awake"),
            }
        } else if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for KeepAwake {
    fn drop(&mut self) {
        self.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_starts_and_stops_its_helper_and_does_nothing_when_nothing_changes() {
        let awake = KeepAwake::with("/bin/sleep", vec!["60".into()]);
        assert!(!awake.is_on());
        awake.set(true);
        assert!(awake.is_on());
        awake.set(true);
        assert!(awake.is_on(), "asking again does not start a second one");
        awake.set(false);
        assert!(!awake.is_on());
        awake.set(false);
    }

    #[test]
    fn a_helper_that_ends_by_itself_is_noticed() {
        let awake = KeepAwake::with("/bin/sleep", vec!["0".into()]);
        awake.set(true);
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(!awake.is_on());
        awake.set(true);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    #[test]
    fn a_program_that_is_missing_is_only_logged() {
        let awake = KeepAwake::with("/no/such/program", vec![]);
        awake.set(true);
        assert!(!awake.is_on());
    }
}
