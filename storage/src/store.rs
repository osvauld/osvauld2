use std::path::Path;
use std::sync::Arc;

use redb::{Database, TableDefinition};

use crate::error::StorageError;

// One keyspace for everything. The hierarchical path lives entirely in the key
// ("space/page/layer/shard"); redb keeps keys sorted, so prefix/range scans are
// available to add later without a schema change.
const TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("store");

#[derive(Clone)]
pub struct Store {
    db: Arc<Database>,
}

impl Store {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        Self::init(Database::create(path)?)
    }

    pub fn open_in_memory() -> Result<Self, StorageError> {
        let db = Database::builder().create_with_backend(redb::backends::InMemoryBackend::new())?;
        Self::init(db)
    }

    // Open an existing store without creating the file or running init. Used to peek at a
    // db we don't own (e.g. another account's label); only reads are performed on it.
    pub fn open_readonly<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        Ok(Self { db: Arc::new(Database::open(path)?) })
    }

    // Create the table eagerly so a read before the first write doesn't hit a
    // missing-table error.
    fn init(db: Database) -> Result<Self, StorageError> {
        let txn = db.begin_write()?;
        txn.open_table(TABLE)?;
        txn.commit()?;
        Ok(Self { db: Arc::new(db) })
    }

    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(TABLE)?;
        Ok(table.get(key)?.map(|value| value.value().to_vec()))
    }

    pub fn put(&self, key: &str, value: &[u8]) -> Result<(), StorageError> {
        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(TABLE)?;
            table.insert(key, value)?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn delete(&self, key: &str) -> Result<(), StorageError> {
        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(TABLE)?;
            table.remove(key)?;
        }
        txn.commit()?;
        Ok(())
    }

    /// Every key that begins with `prefix`, in sorted order. redb keeps keys
    /// ordered, so this is a range scan from `prefix` that stops at the first key
    /// that no longer shares it — the enumeration the keyspace comment anticipated.
    pub fn list_prefixed(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(TABLE)?;
        let mut keys = Vec::new();
        for item in table.range(prefix..)? {
            let (key, _value) = item?;
            let key = key.value();
            if !key.starts_with(prefix) {
                break;
            }
            keys.push(key.to_string());
        }
        Ok(keys)
    }
}

#[cfg(test)]
mod tests;
