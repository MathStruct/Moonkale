//! Records: typed values with a table, a version and serde JSON underneath.
//!
//! Every stored value is an envelope `{"v": <version>, "data": …}`, so a
//! later build can migrate an old record when it reads it, and an older build
//! refuses a newer record instead of misreading it.

use crate::{Batch, Key, StateError, StateStore};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// A kind of record: which table it lives in and which version of its shape
/// this build writes.
pub trait Record: Serialize + DeserializeOwned {
    const TABLE: &'static str;
    const VERSION: u32;

    /// Turn the `data` of an older version into this one. Default: read it
    /// as is (fine while fields are only added with `#[serde(default)]`).
    fn migrate(version: u32, data: serde_json::Value) -> Result<Self, String> {
        let _ = version;
        serde_json::from_value(data).map_err(|e| e.to_string())
    }
}

#[derive(Serialize, Deserialize)]
struct Envelope<T> {
    v: u32,
    data: T,
}

/// Encode a record for [`Batch::put`].
pub fn encode<R: Record>(record: &R) -> Vec<u8> {
    serde_json::to_vec(&Envelope {
        v: R::VERSION,
        data: record,
    })
    .expect("records serialize")
}

/// Decode a stored record, migrating older versions.
pub fn decode<R: Record>(bytes: &[u8]) -> Result<R, StateError> {
    let env: Envelope<serde_json::Value> =
        serde_json::from_slice(bytes).map_err(|e| StateError::Decode {
            table: R::TABLE,
            message: e.to_string(),
        })?;
    if env.v > R::VERSION {
        return Err(StateError::TooNew {
            table: R::TABLE,
            found: env.v,
            supported: R::VERSION,
        });
    }
    if env.v == R::VERSION {
        serde_json::from_value(env.data).map_err(|e| StateError::Decode {
            table: R::TABLE,
            message: e.to_string(),
        })
    } else {
        R::migrate(env.v, env.data).map_err(|message| StateError::Decode {
            table: R::TABLE,
            message,
        })
    }
}

/// Records over any [`StateStore`].
#[derive(Clone, Copy)]
pub struct Typed<'a> {
    pub store: &'a dyn StateStore,
}

impl<'a> Typed<'a> {
    pub fn new(store: &'a dyn StateStore) -> Self {
        Self { store }
    }

    pub fn get<R: Record>(&self, key: &Key) -> Result<Option<R>, StateError> {
        self.store
            .get(R::TABLE, key.as_bytes())?
            .map(|b| decode(&b))
            .transpose()
    }

    /// Every record whose key starts with `prefix`, in key order, with its key.
    pub fn scan<R: Record>(&self, prefix: &Key) -> Result<Vec<(Vec<u8>, R)>, StateError> {
        self.store
            .scan(R::TABLE, prefix.as_bytes())?
            .into_iter()
            .map(|(k, v)| decode(&v).map(|r| (k, r)))
            .collect()
    }

    pub fn put<R: Record>(&self, key: &Key, record: &R) -> Result<(), StateError> {
        self.store
            .write(Batch::new().put(R::TABLE, key.as_bytes(), encode(record)))
    }
}

/// Add a record to a batch (several records, several tables, one commit).
pub fn put_in<R: Record>(batch: Batch, key: &Key, record: &R) -> Batch {
    batch.put(R::TABLE, key.as_bytes(), encode(record))
}
