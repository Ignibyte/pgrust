//! pgrust extension (the `init` feature, no C counterpart): a data directory
//! and a share directory without PostgreSQL installed.
//!
//! pgrust has no bootstrap mode (`--boot` is refused), so it cannot make a
//! cluster itself. This module carries one instead: a pristine cluster made by
//! stock PostgreSQL 18.6's `initdb` (C locale, UTF8, UTC, superuser
//! `postgres`, trust authentication), and the runtime share directory pgrust
//! reads through `PGRUST_PGSHAREDIR` / `PGRUST_TZDIR`. Both are gzip'd tar
//! archives under `pristine/`, written by `scripts/rustal-pristine.sh` together
//! with `SHA256SUMS`, which every unpack checks the embedded bytes against.
//!
//! The server is started on the result as usual:
//! `PGRUST_PGSHAREDIR=<share> PGRUST_TZDIR=<share>/timezone postgres -D <data>`.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// The pristine cluster, `cluster.tar.gz`.
const CLUSTER: &[u8] = include_bytes!("../pristine/cluster.tar.gz");
/// The runtime share directory, `share.tar.gz`.
const SHARE: &[u8] = include_bytes!("../pristine/share.tar.gz");
/// `shasum -a 256` lines for both archives, as the maintainer script wrote them.
const SUMS: &str = include_str!("../pristine/SHA256SUMS");

/// Why an unpack refused or failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum InitError {
    /// The target exists and is not an empty directory; nothing was written.
    NotEmpty(PathBuf),
    /// The embedded archive does not match the checksum `SHA256SUMS` records
    /// for it: the build carries archives the maintainer script did not write
    /// together.
    Checksum {
        /// The archive's file name.
        archive: &'static str,
        /// What `SHA256SUMS` records (empty when it records nothing).
        recorded: String,
        /// The embedded bytes' digest.
        actual: String,
    },
    /// Reading the target or writing the files failed.
    Io(PathBuf, io::Error),
}

impl fmt::Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotEmpty(dir) => {
                write!(f, "{} exists and is not an empty directory", dir.display())
            }
            Self::Checksum { archive, recorded, actual } => write!(
                f,
                "the embedded {archive} has sha256 {actual}, but SHA256SUMS records {}",
                if recorded.is_empty() { "nothing" } else { recorded }
            ),
            Self::Io(path, e) => write!(f, "{}: {e}", path.display()),
        }
    }
}

impl std::error::Error for InitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(_, e) => Some(e),
            _ => None,
        }
    }
}

/// The PostgreSQL version the embedded archives were made by, as the
/// maintainer script recorded it (`postgres (PostgreSQL) 18.6 …`).
pub fn pristine_postgres_version() -> &'static str {
    SUMS.lines()
        .find_map(|l| l.strip_prefix("# PostgreSQL: "))
        .unwrap_or("")
}

/// The sha256 `SHA256SUMS` records for the share archive: what an embedding
/// binary can name an installed share directory by, so a fork bump that
/// changes it installs a new one beside the old.
pub fn share_sha256() -> &'static str {
    recorded("share.tar.gz")
}

/// Unpacks the pristine cluster into `dir`, which must be absent or an empty
/// directory, and leaves it `0700` as the server requires. The server starts
/// on the result with superuser `postgres` and trust authentication: who may
/// connect, and how, is the caller's to set (`pg_hba.conf`, the socket).
pub fn init_data_dir(dir: &Path) -> Result<(), InitError> {
    unpack("cluster.tar.gz", CLUSTER, dir)?;
    restrict(dir)
}

/// Unpacks the runtime share directory into `dir`, which must be absent or an
/// empty directory. Point `PGRUST_PGSHAREDIR` at it and `PGRUST_TZDIR` at its
/// `timezone`.
pub fn install_share_dir(dir: &Path) -> Result<(), InitError> {
    unpack("share.tar.gz", SHARE, dir)
}

fn recorded(archive: &str) -> &'static str {
    SUMS.lines()
        .filter(|l| !l.starts_with('#'))
        .find_map(|l| {
            let (sum, name) = l.split_once("  ")?;
            (name.trim() == archive).then_some(sum.trim())
        })
        .unwrap_or("")
}

fn unpack(archive: &'static str, bytes: &[u8], dir: &Path) -> Result<(), InitError> {
    let actual: String = Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
    let recorded = recorded(archive);
    if recorded != actual {
        return Err(InitError::Checksum { archive, recorded: recorded.to_owned(), actual });
    }
    match fs::read_dir(dir) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                return Err(InitError::NotEmpty(dir.to_path_buf()));
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(dir).map_err(|e| InitError::Io(dir.to_path_buf(), e))?;
        }
        Err(e) => return Err(InitError::Io(dir.to_path_buf(), e)),
    }
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    tar.set_preserve_permissions(true);
    tar.set_preserve_mtime(false);
    tar.unpack(dir).map_err(|e| InitError::Io(dir.to_path_buf(), e))
}

#[cfg(unix)]
fn restrict(dir: &Path) -> Result<(), InitError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
        .map_err(|e| InitError::Io(dir.to_path_buf(), e))
}

#[cfg(not(unix))]
fn restrict(_dir: &Path) -> Result<(), InitError> {
    Ok(())
}
