//! Filesystem watcher for managed folders. New files surface on the channel
//! and the daemon can call ScanStage::discover on the parent dir to pick them up.

use anyhow::Result;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc;

#[allow(dead_code)]
pub struct FolderWatcher {
    _watcher: RecommendedWatcher,
    pub events: mpsc::Receiver<PathBuf>,
}

#[allow(dead_code)]
impl FolderWatcher {
    pub fn watch(roots: Vec<PathBuf>) -> Result<Self> {
        let (tx, rx) = mpsc::channel::<PathBuf>();
        let mut watcher =
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                if let Ok(ev) = res {
                    if matches!(ev.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                        for p in ev.paths {
                            let _ = tx.send(p);
                        }
                    }
                }
            })?;
        for root in &roots {
            watcher.watch(root, RecursiveMode::Recursive)?;
        }
        Ok(Self {
            _watcher: watcher,
            events: rx,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use tempfile::tempdir;

    /// Drain events for up to `deadline_ms`, returning whatever paths arrived.
    fn drain(rx: &mpsc::Receiver<PathBuf>, deadline_ms: u64) -> Vec<PathBuf> {
        let deadline = Instant::now() + Duration::from_millis(deadline_ms);
        let mut out = Vec::new();
        while Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(p) => out.push(p),
                Err(_) => continue,
            }
        }
        out
    }

    #[test]
    fn watcher_emits_for_new_files() {
        let d = tempdir().unwrap();
        let watcher = FolderWatcher::watch(vec![d.path().to_path_buf()]).unwrap();

        // Give the OS a moment to register the watch before mutating.
        std::thread::sleep(Duration::from_millis(100));
        let new_file = d.path().join("new.jpg");
        std::fs::write(&new_file, b"fake").unwrap();

        let events = drain(&watcher.events, 1500);
        assert!(
            events.iter().any(|p| p.file_name() == new_file.file_name()),
            "expected event for {:?}, got {:?}",
            new_file,
            events
        );
    }

    #[test]
    fn watcher_with_no_roots_is_silent() {
        let watcher = FolderWatcher::watch(vec![]).unwrap();
        let events = drain(&watcher.events, 200);
        assert!(events.is_empty());
    }
}
