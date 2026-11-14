//! The Wi-Fi networks the recovery keeps between starts, and how they are
//! written down.
//!
//! A recovery cannot ask the phone's own system to remember a network: the
//! framework that would is not running. What it can do is write the same
//! thing down for itself, on `/metadata` — a real filesystem that is there
//! before the data partition opens, and one the recovery keeps read-only
//! except while it writes.
//!
//! The record is a password written down. What that is worth is said where
//! the switch that offers it is drawn, and here it is only made as narrow as
//! this program can make it: the file is created (and kept) readable by its
//! owner alone, a password is never put in the kernel log or in an error
//! message, and a start cut off mid-write leaves the old record or the new
//! one and never half of either.
//!
//! What is written *where* lives in [`skyward_net::supplicant`], beside the
//! configuration generated from it at every start: the format and the
//! generated `wpa_supplicant.conf` are one thing, and this module only owns
//! the file's mode, its atomic replacement, and the read/write around the
//! metadata mount.

use anyhow::{bail, Context, Result};
use skyward_net::supplicant::{self, Saved};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

/// Where the networks whose passwords are kept are written. `/metadata` is
/// there before the data partition opens, which is why a phone can join a
/// kept network without anyone unlocking anything first.
pub const FILE: &str = "/metadata/skyward/wlan";

/// The folder the record lives in, beside the recovery's settings.
const FOLDER: &str = "/metadata/skyward";

/// What a write goes into first, to be moved over the record whole.
const WRITING: &str = "/metadata/skyward/wlan.new";

/// The record holds passwords: only the recovery's own user may read it.
const FILE_MODE: u32 = 0o600;

/// And only it may list the folder it is in.
const FOLDER_MODE: u32 = 0o700;

/// The networks kept, read from where they are written.
///
/// A record that cannot be read is no record: none is joined by itself, and
/// why goes to the kernel log. This is not a failure worth stopping the WLAN
/// for — a phone whose `/metadata` will not mount still has a radio, and a
/// network joined by hand still works, it is only not remembered.
pub fn load() -> Vec<Saved> {
    match with_metadata(|| read_at(Path::new(FILE))) {
        Ok(kept) => kept,
        Err(e) => {
            let _ = skyward_sys::kmsg(&format!("the networks kept were not read: {e:#}"));
            Vec::new()
        }
    }
}

/// Write `kept` where a later start finds it, or take the record away when
/// there is none to keep.
///
/// The metadata mount is made writable for the length of the write alone and
/// read-only again after it, whether the write worked or not, through the
/// recovery's one gate for that ([`crate::settings::with_writable_metadata`]);
/// the settings file is written through the same gate, so a settings write
/// and a record write never see the mount in the other's state. The write
/// goes into a file beside the record, is flushed, and is moved over it
/// whole.
pub fn save(kept: &[Saved]) -> Result<()> {
    with_writable_metadata(|| write_at(Path::new(FOLDER), Path::new(FILE), Path::new(WRITING), kept))
}

/// What the file holds, read from `path`. A file that is not there is no
/// networks kept rather than a failure.
fn read_at(path: &Path) -> Result<Vec<Saved>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
    };
    Ok(supplicant::saved_read(&text))
}

/// `kept` into `writing`, flushed, then moved over `file`, with `folder`
/// made private to its owner. No password reaches a context or an error
/// message: only paths and counts are said.
fn write_at(folder: &Path, file: &Path, writing: &Path, kept: &[Saved]) -> Result<()> {
    if kept.len() > supplicant::SAVED_MOST {
        bail!("{} networks are more than the {} kept", kept.len(), supplicant::SAVED_MOST);
    }
    if kept.is_empty() {
        // Nothing kept is the record not being there: a file holding no
        // networks is one an earlier version would still have to parse.
        let removed = match fs::remove_file(file) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e).with_context(|| format!("take away {}", file.display())),
        };
        removed?;
        // The removal is on the filesystem too, and a start cut off before
        // it reached the disk would bring the wiped record back.
        sync_folder(folder);
        return Ok(());
    }
    let wrote = (|| -> Result<()> {
        fs::create_dir_all(folder).with_context(|| format!("make {}", folder.display()))?;
        fs::set_permissions(folder, fs::Permissions::from_mode(FOLDER_MODE)).with_context(|| format!("close {} to others", folder.display()))?;
        let mut output = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(FILE_MODE)
            .open(writing)
            .with_context(|| format!("make {}", writing.display()))?;
        // `mode` is only what a new file is made with; one an earlier
        // failure left is given the mode again.
        output.set_permissions(fs::Permissions::from_mode(FILE_MODE)).with_context(|| format!("close {} to others", writing.display()))?;
        output.write_all(supplicant::saved_text(kept).as_bytes()).with_context(|| format!("write {}", writing.display()))?;
        output.sync_all().with_context(|| format!("flush {}", writing.display()))?;
        drop(output);
        fs::rename(writing, file).with_context(|| format!("move {} over {}", writing.display(), file.display()))?;
        sync_folder(folder);
        Ok(())
    })();
    if wrote.is_err() {
        // A half-written file beside the record would be read by nothing,
        // and is not worth leaving where a later write could pick it up.
        let _ = fs::remove_file(writing);
    }
    wrote
}

/// Make the rename or the removal itself reach the disk. A folder that
/// cannot be opened leaves the write standing: what was written is complete
/// already, and failing the whole thing over the folder's own flush would
/// say a record was not kept when it is.
fn sync_folder(folder: &Path) {
    if let Ok(dir) = File::open(folder) {
        let _ = dir.sync_all();
    }
}

// The metadata mount is the recovery's one shared resource: the settings are
// written through it too, and a remount to read-only by one writer while the
// other is still writing would either fail that write or, worse, let it be
// lost. `crate::settings` owns the gate (mounting /metadata when it is not
// mounted, a mutex around the pair, and the read-only state put back even
// when the work fails), so both files go through it and neither remounts
// anything of its own.
use crate::settings::{with_metadata, with_writable_metadata};

#[cfg(test)]
mod tests {
    use super::*;
    use skyward_net::supplicant::Security;

    fn kept(ssid: &[u8], security: Security, password: &str) -> Saved {
        Saved { ssid: ssid.to_vec(), name: String::from_utf8_lossy(ssid).into_owned(), security, password: password.into() }
    }

    /// A record written where the test can reach it, with the same code the
    /// phone's record is written by.
    fn write(dir: &Path, kept: &[Saved]) -> Result<()> {
        write_at(&dir.join("skyward"), &dir.join("skyward/wlan"), &dir.join("skyward/wlan.new"), kept)
    }

    fn read(dir: &Path) -> Result<Vec<Saved>> {
        read_at(&dir.join("skyward/wlan"))
    }

    #[test]
    fn the_record_is_written_for_its_owner_alone_and_reads_back_whole() {
        let dir = tempfile::tempdir().unwrap();
        let all = [kept("Skyward客厅".as_bytes(), Security::Psk, "pass\"word"), kept(b"Skyward-5G", Security::Sae, "短いパスワード")];
        write(dir.path(), &all).unwrap();
        let path = dir.path().join("skyward/wlan");
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the password is readable by its owner alone");
        assert_eq!(read(dir.path()).unwrap(), all, "what was written reads back as it was written");
    }

    #[test]
    fn the_folder_is_closed_to_everyone_but_its_owner() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &[kept(b"Skyward", Security::Psk, "12345678")]).unwrap();
        let mode = fs::metadata(dir.path().join("skyward")).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "the records are listed by their owner alone");
    }

    #[test]
    fn a_write_is_moved_into_place_whole_and_leaves_nothing_beside_it() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &[kept(b"First", Security::Psk, "11111111")]).unwrap();
        write(dir.path(), &[kept(b"Second", Security::Psk, "22222222")]).unwrap();
        assert_eq!(read(dir.path()).unwrap(), [kept(b"Second", Security::Psk, "22222222")]);
        assert!(!dir.path().join("skyward/wlan.new").exists(), "no half-written record is left beside the record");
    }

    #[test]
    fn a_record_of_nothing_takes_the_file_away() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &[kept(b"Skyward", Security::Psk, "12345678")]).unwrap();
        write(dir.path(), &[]).unwrap();
        assert!(!dir.path().join("skyward/wlan").exists(), "nothing kept is the record not being there");
        assert!(read(dir.path()).unwrap().is_empty(), "which reads as no networks kept");
    }

    #[test]
    fn a_write_that_fails_leaves_the_old_record_and_says_nothing_of_the_password() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &[kept(b"Old", Security::Psk, "11111111")]).unwrap();
        // A regular file where the folder has to be: the write cannot start,
        // and what it would have written is not said.
        let blocked = dir.path().join("blocked");
        fs::write(&blocked, b"not a folder").unwrap();
        let password = "passphrase-not-to-be-logged";
        let writing = blocked.join("wlan.new");
        let record = blocked.join("wlan");
        let why = write_at(&blocked, &record, &writing, &[kept(b"New", Security::Psk, password)]).unwrap_err();
        let said = format!("{why:#}");
        assert!(said.contains("make"), "the reason is the folder, not the record: {said}");
        assert!(!said.contains(password), "the password is not in the reason: {said}");
        assert_eq!(read(dir.path()).unwrap(), [kept(b"Old", Security::Psk, "11111111")], "the old record stands");
        assert!(!writing.exists(), "nothing half-written is left");
    }

    #[test]
    fn more_networks_than_the_record_holds_are_refused_before_anything_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let many: Vec<Saved> = (0..=supplicant::SAVED_MOST).map(|n| kept(format!("Net{n}").as_bytes(), Security::Psk, "12345678")).collect();
        let why = write_at(dir.path(), &dir.path().join("wlan"), &dir.path().join("wlan.new"), &many).unwrap_err();
        assert!(why.to_string().contains("more than"), "{why:#}");
        assert!(!dir.path().join("wlan").exists(), "nothing was written");
    }

    #[test]
    fn what_a_page_is_shown_of_a_kept_network_is_never_its_password() {
        let network = kept(b"Skyward", Security::Psk, "secret-password");
        let shown = network.shown();
        assert_eq!(shown.name, "Skyward");
        assert_eq!(shown.security, Security::Psk);
        assert!(!format!("{shown:?}").contains("secret-password"), "a page's view holds no password");
    }
}
