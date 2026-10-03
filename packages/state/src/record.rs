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
///
/// A record of this build's version is read straight into its type, not
/// through `serde_json::Value` — which would turn a `u128` (an event id)
/// into a float.
pub fn decode<R: Record>(bytes: &[u8]) -> Result<R, StateError> {
    #[derive(Deserialize)]
    struct Version {
        v: u32,
    }
    let bad = |e: serde_json::Error| StateError::Decode {
        table: R::TABLE,
        message: e.to_string(),
    };
    let v = serde_json::from_slice::<Version>(bytes).map_err(bad)?.v;
    if v > R::VERSION {
        return Err(StateError::TooNew {
            table: R::TABLE,
            found: v,
            supported: R::VERSION,
        });
    }
    if v == R::VERSION {
        serde_json::from_slice::<Envelope<R>>(bytes)
            .map(|env| env.data)
            .map_err(bad)
    } else {
        let env: Envelope<serde_json::Value> = serde_json::from_slice(bytes).map_err(bad)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Id(u128);

    impl Record for Id {
        const TABLE: &'static str = "ids";
        const VERSION: u32 = 1;
    }

    #[test]
    fn a_u128_survives_the_envelope() {
        let id = Id(1_759_400_000_000u128 << 64 | 0xdead_beef_dead_beef);
        assert_eq!(decode::<Id>(&encode(&id)).unwrap(), id);
    }
}
