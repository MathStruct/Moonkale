//! Bounded explicit session exchange, independent of persistence engines.
use serde::de::DeserializeOwned;

/// Maximum UTF-8 size of a portable session description.
pub const MAX_BYTES: usize = 4 * 1024 * 1024;

/// Read a versioned session bundle. Serde's default recursion bound remains on.
/// Unknown versions are refused before deserializing application state.
pub fn parse<T: DeserializeOwned>(text: &str) -> Result<T, &'static str> {
    if text.len() > MAX_BYTES {
        return Err("session bundle too large");
    }
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "invalid session bundle")?;
    if value.get("bundle_version").and_then(|v| v.as_u64()) != Some(1) {
        return Err("unsupported session bundle version");
    }
    serde_json::from_value(value).map_err(|_| "invalid session bundle")
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_unversioned_future_oversize_and_deep_input() {
        use super::*;
        for input in [
            "{}".to_string(),
            "{\"bundle_version\":2}".into(),
            " ".repeat(MAX_BYTES + 1),
            format!("{}0{}", "[".repeat(200), "]".repeat(200)),
        ] {
            assert!(parse::<serde_json::Value>(&input).is_err());
        }
        assert!(parse::<serde_json::Value>("{\"bundle_version\":1}").is_ok());
    }
}
