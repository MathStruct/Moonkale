//! Typed values — what a database cell, a property or a row holds.
//!
//! Milestone 2 introduced the minimum for SQL rows; Milestone 18 phase 6.2
//! ([[ADR-0015 The core model]]) put [`Properties`] on every node and edge —
//! a factor's parameters (Lenticulum), a declaration's hash (Sophia), a
//! row's columns. Date/time, node references and bounded numeric vectors
//! are shared typed cells (WP-15).

use crate::NodeId;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A node's or edge's properties, by name, sorted (so they compare and
/// serialize the same everywhere).
pub type Properties = std::collections::BTreeMap<String, Value>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    /// UTC instant, retaining the original offset when supplied.
    #[serde(rename = "DateTime")]
    DateTime(DateTimeCell),
    /// A source-known graph identity, never inferred from arbitrary strings.
    #[serde(rename = "Ref")]
    Ref {
        node: NodeId,
        label: String,
    },
    /// Finite numeric samples, at most 4096 elements.
    #[serde(rename = "Vector")]
    Vector(NumericVector),
    /// Binary data, shown by size; the bytes are fetched on demand later.
    Bytes {
        len: u64,
    },
}

/// Floats compare by their bits, so `Value` is `Eq` (a NaN equals itself) and
/// nodes carrying properties stay `Eq`.
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        use Value::*;
        match (self, other) {
            (Null, Null) => true,
            (Bool(a), Bool(b)) => a == b,
            (Int(a), Int(b)) => a == b,
            (Float(a), Float(b)) => a.to_bits() == b.to_bits(),
            (Text(a), Text(b)) => a == b,
            (DateTime(a), DateTime(b)) => a == b,
            (Ref { node: a, label: x }, Ref { node: b, label: y }) => a == b && x == y,
            (Vector(a), Vector(b)) => a == b,
            (Bytes { len: a }, Bytes { len: b }) => a == b,
            _ => false,
        }
    }
}

impl Eq for Value {}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => f.write_str("NULL"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(x) => write!(f, "{x}"),
            Value::Text(s) => f.write_str(s),
            Value::DateTime(d) => write!(f, "{}", d.utc.to_rfc3339()),
            Value::Ref { label, .. } => f.write_str(label),
            Value::Vector(v) => write!(f, "{:?}", v.as_slice()),
            Value::Bytes { len } => write!(f, "<{len} bytes>"),
        }
    }
}

/// A UTC instant with an optional original offset in seconds east of UTC.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DateTimeCell {
    utc: chrono::DateTime<chrono::Utc>,
    offset: Option<i32>,
}
impl DateTimeCell {
    /// Construct a validated instant. Offsets must be less than 24 hours.
    pub fn new(seconds: i64, nanos: u32, offset: Option<i32>) -> Option<Self> {
        if offset.is_some_and(|n| !(-86399..=86399).contains(&n)) || nanos >= 1_000_000_000 {
            return None;
        }
        Some(Self {
            utc: chrono::DateTime::from_timestamp(seconds, nanos)?,
            offset,
        })
    }
    /// Parse ISO date/time text, capped before parsing. Naive SQL dates and
    /// timestamps use UTC; time-only values have no instant and remain text.
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() > 128 {
            return None;
        }
        if let Ok(d) = chrono::DateTime::parse_from_rfc3339(text) {
            return Self::new(
                d.timestamp(),
                d.timestamp_subsec_nanos(),
                Some(d.offset().local_minus_utc()),
            );
        }
        let naive = chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f")
            .ok()
            .or_else(|| {
                chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
                    .ok()?
                    .and_hms_opt(0, 0, 0)
            })?;
        Self::new(
            naive.and_utc().timestamp(),
            naive.and_utc().timestamp_subsec_nanos(),
            None,
        )
    }
    /// Chronological order, including subsecond precision.
    pub fn compare(&self, other: &Self) -> std::cmp::Ordering {
        self.utc.cmp(&other.utc)
    }
    /// Seconds since the Unix epoch.
    pub fn seconds(&self) -> i64 {
        self.utc.timestamp()
    }
    /// Original offset, if the source supplied one.
    pub fn offset(&self) -> Option<i32> {
        self.offset
    }
    /// Locale-specific ordering and punctuation; uses the original wall time,
    /// or UTC for sources without offset metadata.
    pub fn display(&self, locale: &str) -> String {
        let shifted = self
            .utc
            .checked_add_signed(chrono::Duration::seconds(self.offset.unwrap_or(0) as i64));
        let offset = if shifted.is_some() { self.offset } else { None };
        let local = shifted.unwrap_or(self.utc);
        let pattern = if locale.starts_with("de") {
            "%d.%m.%Y %H:%M:%S%.f"
        } else if locale.starts_with("zh") {
            "%Y年%m月%d日 %H:%M:%S%.f"
        } else {
            "%Y-%m-%d %H:%M:%S%.f"
        };
        format!(
            "{} {}",
            local.format(pattern),
            offset
                .map(|n| format!(
                    "{}{:02}:{:02}{}",
                    if n < 0 { "-" } else { "+" },
                    n.abs() / 3600,
                    (n.abs() % 3600) / 60,
                    if n.abs() % 60 == 0 {
                        String::new()
                    } else {
                        format!(":{:02}", n.abs() % 60)
                    }
                ))
                .unwrap_or_else(|| "UTC".into())
        )
    }
}
impl<'de> Deserialize<'de> for DateTimeCell {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            utc: chrono::DateTime<chrono::Utc>,
            offset: Option<i32>,
        }
        let w = Wire::deserialize(d)?;
        Self::new(w.utc.timestamp(), w.utc.timestamp_subsec_nanos(), w.offset)
            .ok_or_else(|| serde::de::Error::custom("invalid date/time"))
    }
}

/// A bounded finite f64 vector. f32 inputs widen without losing information.
#[derive(Clone, Debug, Serialize)]
#[serde(transparent)]
pub struct NumericVector(Vec<f64>);
impl NumericVector {
    /// Maximum samples per cell, enforced by constructors and deserialization.
    pub const MAX_LEN: usize = 4096;
    /// Validate before accepting data.
    pub fn new(values: Vec<f64>) -> Option<Self> {
        (values.len() <= Self::MAX_LEN && values.iter().all(|v| v.is_finite()))
            .then_some(Self(values))
    }
    /// Borrow the validated samples.
    pub fn as_slice(&self) -> &[f64] {
        &self.0
    }
}
impl PartialEq for NumericVector {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && self
                .0
                .iter()
                .zip(&other.0)
                .all(|(a, b)| a.to_bits() == b.to_bits())
    }
}
impl Eq for NumericVector {}
impl<'de> Deserialize<'de> for NumericVector {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Samples;
        impl<'de> serde::de::Visitor<'de> for Samples {
            type Value = NumericVector;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("at most 4096 finite numbers")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some(v) = seq.next_element::<f64>()? {
                    if out.len() == NumericVector::MAX_LEN || !v.is_finite() {
                        return Err(serde::de::Error::custom("invalid numeric vector"));
                    }
                    out.push(v);
                }
                Ok(NumericVector(out))
            }
        }
        d.deserialize_seq(Samples)
    }
}

#[cfg(test)]
mod typed_tests {
    use super::*;
    #[test]
    fn dates_validate_normalize_and_localize() {
        let d = DateTimeCell::parse("2026-10-23T10:30:00+02:00").unwrap();
        assert_eq!(
            d.seconds(),
            DateTimeCell::parse("2026-10-23T08:30:00Z")
                .unwrap()
                .seconds()
        );
        assert_eq!(d.offset(), Some(7200));
        assert!(d.display("de").starts_with("23.10.2026 10:30"));
        assert!(d.display("zh-CN").starts_with("2026年10月23日"));
        assert!(DateTimeCell::parse("2026-02-30").is_none());
        assert!(DateTimeCell::new(0, 0, Some(86400)).is_none());
        assert!(DateTimeCell::new(0, 0, Some(-1800))
            .unwrap()
            .display("en")
            .ends_with("-00:30"));
        assert!(DateTimeCell::new(0, 500_000_000, None)
            .unwrap()
            .display("en")
            .contains("00:00:00.500"));
        let edge = DateTimeCell::new(
            chrono::DateTime::<chrono::Utc>::MAX_UTC.timestamp(),
            0,
            Some(86399),
        )
        .unwrap();
        assert!(edge.display("de").ends_with("UTC"));
    }
    #[test]
    fn vectors_are_bounded_and_finite_on_every_entry() {
        assert!(NumericVector::new(vec![f64::NAN]).is_none());
        assert!(NumericVector::new(vec![0.; 4097]).is_none());
        assert!(
            serde_json::from_str::<NumericVector>(&format!("[{}]", vec!["0"; 4097].join(",")))
                .is_err()
        );
        let v = Value::Vector(NumericVector::new(vec![1., -2., 3.]).unwrap());
        assert_eq!(
            serde_json::from_str::<Value>(&serde_json::to_string(&v).unwrap()).unwrap(),
            v
        );
    }
    #[test]
    fn wire_names_and_existing_encodings_are_stable() {
        assert_eq!(
            serde_json::to_string(&Value::Int(7)).unwrap(),
            r#"{"Int":7}"#
        );
        let id = NodeId::derive(&crate::SourceId::new("test"), "target");
        let values = [
            Value::Ref {
                node: id,
                label: "target".into(),
            },
            Value::DateTime(DateTimeCell::parse("2026-10-23").unwrap()),
        ];
        for v in values {
            let json = serde_json::to_string(&v).unwrap();
            assert_eq!(serde_json::from_str::<Value>(&json).unwrap(), v);
        }
    }
}
