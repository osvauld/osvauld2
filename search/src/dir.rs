//! tantivy's `Directory` over sealed vault entries.
//!
//! Reads are served from a `RamDirectory` filled from the vault at open — the whole index is
//! held unsealed in memory, which is fine at an item's scale and the thing to revisit if one
//! reaches hundreds of MB. Every write goes to the vault first, then to the RAM copy, so a
//! sealing failure (a locked vault) surfaces as a failed commit rather than an index that
//! reads fine now and is gone after a restart.

use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tantivy::directory::error::{DeleteError, OpenReadError, OpenWriteError};
use tantivy::directory::{
    AntiCallToken, Directory, FileHandle, FileSlice, RamDirectory, TerminatingWrite,
    WatchCallback, WatchHandle, WritePtr,
};
use vault::{Vault, VaultError};

#[derive(Clone)]
pub struct VaultDirectory {
    ram: RamDirectory,
    vault: Vault,
    /// `search/<ws>/<item>/files/` — ends in `/` so one item's prefix never matches another's.
    prefix: String,
}

impl std::fmt::Debug for VaultDirectory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VaultDirectory({})", self.prefix)
    }
}

impl VaultDirectory {
    pub fn load(vault: Vault, prefix: String) -> Result<Self, VaultError> {
        let ram = RamDirectory::create();
        for name in vault.list_entries(&prefix)? {
            if let Some(bytes) = vault.get_entry(&name)? {
                let file = name.strip_prefix(&prefix).unwrap_or(&name);
                ram.atomic_write(Path::new(file), &bytes)
                    .map_err(|e| VaultError::InvalidName(e.to_string()))?;
            }
        }
        Ok(Self { ram, vault, prefix })
    }

    fn entry(&self, path: &Path) -> String {
        format!("{}{}", self.prefix, path.to_string_lossy())
    }

    /// Lock files guard one process's writer; persisting them would leave a stale lock that
    /// refuses every writer after a crash.
    fn persisted(path: &Path) -> bool {
        !path.to_string_lossy().ends_with(".lock")
    }

    fn seal(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        if Self::persisted(path) {
            self.vault
                .put_entry(&self.entry(path), bytes)
                .map_err(io::Error::other)?;
        }
        Ok(())
    }
}

impl Directory for VaultDirectory {
    fn get_file_handle(&self, path: &Path) -> Result<Arc<dyn FileHandle>, OpenReadError> {
        self.ram.get_file_handle(path)
    }

    fn open_read(&self, path: &Path) -> Result<FileSlice, OpenReadError> {
        self.ram.open_read(path)
    }

    fn delete(&self, path: &Path) -> Result<(), DeleteError> {
        self.ram.delete(path)?;
        if Self::persisted(path) {
            self.vault
                .delete_entry(&self.entry(path))
                .map_err(|e| DeleteError::IoError {
                    io_error: Arc::new(io::Error::other(e)),
                    filepath: path.to_path_buf(),
                })?;
        }
        Ok(())
    }

    fn exists(&self, path: &Path) -> Result<bool, OpenReadError> {
        self.ram.exists(path)
    }

    fn open_write(&self, path: &Path) -> Result<WritePtr, OpenWriteError> {
        if self.ram.exists(path).unwrap_or(false) {
            return Err(OpenWriteError::FileAlreadyExists(path.to_path_buf()));
        }
        // Visible at once, as a filesystem would make it: the lock protocol depends on a
        // second `open_write` of the same path failing.
        self.ram
            .atomic_write(path, &[])
            .map_err(|e| OpenWriteError::wrap_io_error(e, path.to_path_buf()))?;
        Ok(BufWriter::new(Box::new(SealedWriter {
            dir: self.clone(),
            path: path.to_path_buf(),
            buf: Vec::new(),
            sealed: None,
        })))
    }

    fn atomic_read(&self, path: &Path) -> Result<Vec<u8>, OpenReadError> {
        self.ram.atomic_read(path)
    }

    fn atomic_write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        self.seal(path, data)?;
        self.ram.atomic_write(path, data)
    }

    fn sync_directory(&self) -> io::Result<()> {
        Ok(())
    }

    fn watch(&self, cb: WatchCallback) -> tantivy::Result<WatchHandle> {
        self.ram.watch(cb)
    }
}

struct SealedWriter {
    dir: VaultDirectory,
    path: PathBuf,
    buf: Vec<u8>,
    /// Length last sealed — a repeated flush with nothing new re-seals nothing.
    sealed: Option<usize>,
}

impl Write for SealedWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.sealed == Some(self.buf.len()) {
            return Ok(());
        }
        self.dir.seal(&self.path, &self.buf)?;
        // `atomic_write` on the RAM side: replacing the empty placeholder `open_write` made.
        self.dir.ram.atomic_write(&self.path, &self.buf)?;
        self.sealed = Some(self.buf.len());
        Ok(())
    }
}

impl TerminatingWrite for SealedWriter {
    fn terminate_ref(&mut self, _: AntiCallToken) -> io::Result<()> {
        self.flush()
    }
}
