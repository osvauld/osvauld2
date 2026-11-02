use std::path::Path;
use std::sync::Arc;

use redb::{Database, ReadableTable, TableDefinition};

use crate::error::StorageError;

// One keyspace for everything. The hierarchical path lives entirely in the key
// ("space/page/layer/shard"); redb keeps keys sorted, so prefix/range scans are
// available to add later without a schema change.
const TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("store");

/// One step of an atomic write, see [`Store::apply`].
#[derive(Debug, Clone)]
pub enum Op {
    Put(String, Vec<u8>),
    Delete(String),
    /// Every key that begins with the prefix.
    DeletePrefix(String),
}

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
        Ok(Self {
            db: Arc::new(Database::open(path)?),
        })
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
        self.apply(&[Op::Put(key.to_string(), value.to_vec())])
    }

    pub fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.apply(&[Op::Delete(key.to_string())])
    }

    pub fn delete_prefix(&self, prefix: &str) -> Result<(), StorageError> {
        self.apply(&[Op::DeletePrefix(prefix.to_string())])
    }

    /// Every op in one transaction: after a crash either all of them happened or none did.
    /// Ops apply in order, so a `Put` after a `DeletePrefix` of its own prefix survives.
    pub fn apply(&self, ops: &[Op]) -> Result<(), StorageError> {
        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(TABLE)?;
            for op in ops {
                match op {
                    Op::Put(key, value) => {
                        table.insert(key.as_str(), value.as_slice())?;
                    }
                    Op::Delete(key) => {
                        table.remove(key.as_str())?;
                    }
                    Op::DeletePrefix(prefix) => {
                        let keys = keys_under(&table, prefix)?;
                        for key in keys {
                            table.remove(key.as_str())?;
                        }
                    }
                }
            }
        }
        txn.commit()?;
        Ok(())
    }

    /// Every key that begins with `prefix`, in sorted order. redb keeps keys
    /// ordered, so this is a range scan from `prefix` that stops at the first key
    /// that no longer shares it.
    pub fn list_prefixed(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(TABLE)?;
        keys_under(&table, prefix)
    }

    /// Every `(key, value)` under `prefix`, in key order — one read transaction, where
    /// `list_prefixed` followed by a `get` per key would be many.
    pub fn scan(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>, StorageError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(TABLE)?;
        let mut rows = Vec::new();
        for item in table.range(prefix..)? {
            let (key, value) = item?;
            if !key.value().starts_with(prefix) {
                break;
            }
            rows.push((key.value().to_string(), value.value().to_vec()));
        }
        Ok(rows)
    }
}

fn keys_under(
    table: &impl ReadableTable<&'static str, &'static [u8]>,
    prefix: &str,
) -> Result<Vec<String>, StorageError> {
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

#[cfg(test)]
mod tests;
