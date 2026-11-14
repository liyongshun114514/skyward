//! The record of the networks the phone keeps, tested on the host.
//!
//! What the recovery writes on `/metadata` is a real file with a real mode,
//! replaced whole by a real rename, and the tests below write it in a
//! temporary folder with the very code the phone runs. The only thing
//! replaced is [`crate::settings`], the recovery's one gate for the metadata
//! mount: here it does the work it is given and mounts nothing, so what is
//! tested is the record — its mode, its replacement, its removal, and the
//! password that never reaches a message — and not the phone's partitions.
//!
//! `remembered.rs` keeps its own tests beside the code they test, and they
//! run from here as `remembered::tests::*`: on the host that is the only way
//! they are run at all, since the module reaches the binary only through
//! `wlan.rs`, which is built for Android alone.

#[path = "../src/remembered.rs"]
mod remembered;

/// The metadata gate, without a mount: `/metadata` is assumed mounted and
/// writable, and the work is done where it stands.
mod settings {
    use anyhow::Result;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// How often the gate has been asked for the mount, and for it
    /// writable, so that a test can say the record goes through it.
    pub(crate) static ASKED: AtomicUsize = AtomicUsize::new(0);
    pub(crate) static WRITTEN: AtomicUsize = AtomicUsize::new(0);

    pub(crate) fn with_metadata<T>(work: impl FnOnce() -> Result<T>) -> Result<T> {
        ASKED.fetch_add(1, Ordering::SeqCst);
        work()
    }

    pub(crate) fn with_writable_metadata<T>(work: impl FnOnce() -> Result<T>) -> Result<T> {
        WRITTEN.fetch_add(1, Ordering::SeqCst);
        work()
    }
}

#[cfg(test)]
mod tests {
    use super::{remembered, settings};
    use std::sync::atomic::Ordering;

    /// Reading what is kept goes through the recovery's one gate for
    /// `/metadata`, and a record that is not there is no networks kept
    /// rather than a failure — which is what a phone whose `/metadata` holds
    /// no record yet joins by itself: nothing.
    #[test]
    fn reading_what_is_kept_goes_through_the_one_metadata_gate() {
        let before = settings::ASKED.load(Ordering::SeqCst);
        let kept = remembered::load();
        assert!(settings::ASKED.load(Ordering::SeqCst) > before, "the record is read on /metadata");
        // The host has no record where the phone keeps one, and a host that
        // does is not this test's to change: only the gate is asserted.
        assert!(kept.len() <= skyward_net::supplicant::SAVED_MOST);
    }

    /// Writing what is kept goes through the writable side of the same gate,
    /// so a record write never races the settings write over the mount.
    ///
    /// An empty record is a removal, and one that is not there is what that
    /// wanted anyway: the test writes nothing on a host, and leaves a host
    /// that does hold a record alone rather than taking it away.
    #[test]
    fn writing_what_is_kept_goes_through_the_writable_metadata_gate() {
        if std::path::Path::new(remembered::FILE).exists() {
            eprintln!("{} is on this host; it is left as it is", remembered::FILE);
            return;
        }
        let before = settings::WRITTEN.load(Ordering::SeqCst);
        remembered::save(&[]).expect("nothing kept is a record that is not there");
        assert!(settings::WRITTEN.load(Ordering::SeqCst) > before, "the record is written through the gate");
        assert!(!std::path::Path::new(remembered::FILE).exists(), "nothing kept leaves no record");
    }
}
