//! Our own saves in flight, for the QML singletons that also watch their files.
//!
//! A file watcher sees our own writes too. While a save is still queued on the background writer,
//! the file on disk is older than the state in memory, so reloading it would undo the user's last
//! change. The singletons skip reloads while [`SaveTracker::pending`] and reload once the writes
//! settle (an external edit made meanwhile is picked up then).
//!
//! The writer drops the callback of a write that a newer one for the same file replaced, so the
//! tracker only compares the newest sequence number queued with the newest one finished.

/// Sequence numbers of queued and finished saves.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SaveTracker {
    queued: u64,
    done: u64,
    /// A reload was skipped while saves were pending: do it once they settle.
    pub reload_wanted: bool,
}

impl SaveTracker {
    /// A save was queued: its sequence number, for [`SaveTracker::finished`].
    pub fn queue(&mut self) -> u64 {
        self.queued += 1;
        self.queued
    }

    /// Save `seq` finished (or failed).
    pub fn finished(&mut self, seq: u64) {
        self.done = self.done.max(seq);
    }

    /// Whether a queued save hasn't finished yet.
    #[must_use]
    pub fn pending(&self) -> bool {
        self.done < self.queued
    }

    /// Whether a skipped reload should run now (and forgets it).
    pub fn take_reload(&mut self) -> bool {
        if self.pending() {
            return false;
        }
        std::mem::take(&mut self.reload_wanted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_replaced_save_still_settles() {
        let mut saves = SaveTracker::default();
        assert!(!saves.pending());
        let first = saves.queue();
        let second = saves.queue();
        saves.reload_wanted = true;
        assert!(saves.pending());
        assert!(!saves.take_reload());
        // The writer dropped the first callback: only the second finishes.
        saves.finished(second);
        assert!(!saves.pending());
        saves.finished(first);
        assert!(!saves.pending());
        assert!(saves.take_reload());
        assert!(!saves.take_reload());
    }
}
