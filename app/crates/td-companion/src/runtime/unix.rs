//! Owner-only files on Linux source builds: a 0700 folder and a 0600 file, owned by the user.
use std::{
    fs::{DirBuilder, OpenOptions},
    io::{self, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
};

fn owned_by_user(meta: &std::fs::Metadata) -> bool {
    // SAFETY: geteuid has no preconditions.
    meta.uid() == unsafe { libc::geteuid() }
}

/// Creates the folder 0700, or secures an existing one. Refuses a symlink and a folder another
/// account owns.
pub(super) fn secure_dir(dir: &Path) -> io::Result<()> {
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match DirBuilder::new().mode(0o700).create(dir) {
        Err(e) if e.kind() != io::ErrorKind::AlreadyExists => return Err(e),
        _ => {}
    }
    let meta = std::fs::symlink_metadata(dir)?;
    if meta.file_type().is_symlink() {
        return Err(io::Error::other("it is a symbolic link"));
    }
    if !meta.is_dir() {
        return Err(io::Error::other("it is not a folder"));
    }
    if !owned_by_user(&meta) {
        return Err(io::Error::other(
            "another account owns it. Delete the folder or make yourself its owner",
        ));
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

/// Writes through a private temporary file and a rename. A failed rename leaves the old file;
/// only a file that fails verification is deleted.
pub(super) fn write_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp = path.with_file_name(format!(".companion.json.{}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&temp);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temp)?;
    let moved = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp, path)
    })();
    if moved.is_err() {
        let _ = std::fs::remove_file(&temp);
        return moved;
    }
    let verified = verify_file(path);
    if verified.is_err() {
        let _ = std::fs::remove_file(path);
    }
    verified
}

pub(super) fn verify_file(path: &Path) -> io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if !meta.is_file() || !owned_by_user(&meta) || meta.mode() & 0o077 != 0 {
        return Err(io::Error::other("its permissions are not owner-only"));
    }
    Ok(())
}
