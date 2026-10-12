//! Shared local file URI identity and folder-boundary validation.
fn decode(uri: &str) -> Option<String> {
    let encoded = uri.strip_prefix("file://")?;
    let encoded = encoded.strip_prefix("localhost").unwrap_or(encoded);
    if !encoded.starts_with('/') || encoded.contains(['?', '#']) {
        return None;
    }
    let mut bytes = Vec::new();
    let mut input = encoded.as_bytes().iter().copied();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let high = (input.next()? as char).to_digit(16)?;
            let low = (input.next()? as char).to_digit(16)?;
            bytes.push((high * 16 + low) as u8);
        } else {
            bytes.push(byte);
        }
    }
    let path = String::from_utf8(bytes).ok()?;
    Some(path)
}

fn encode(path: &str) -> String {
    let mut uri = String::from("file://");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~:".contains(&byte) {
            uri.push(byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}
pub(crate) fn file_uri(root: &str, path: &str) -> String {
    encode(&format!("{}/{path}", root.trim_end_matches('/')))
}
pub(crate) fn key(uri: &str) -> String {
    decode(uri)
        .map(|path| encode(&path))
        .unwrap_or_else(|| uri.to_string())
}
pub(crate) fn relative_file(root: &str, uri: &str) -> Option<String> {
    let path = decode(uri)?;
    let prefix = format!("{}/", root.trim_end_matches('/'));
    let relative = path.strip_prefix(&prefix)?;
    if relative
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == ".." || part.contains(['\\', '\0']))
    {
        return None;
    }
    Some(relative.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_share_identity_and_decoded_folder_boundaries() {
        let root = "/tmp/My Project/Ü";
        let uri = file_uri(root, "foo+bar #é.rs");
        assert_eq!(relative_file(root, &uri).as_deref(), Some("foo+bar #é.rs"));
        assert_eq!(
            key("file:///tmp/foo+bar.rs"),
            key("file://localhost/tmp/foo%2bbar.rs")
        );
        assert_eq!(key("file:///tmp/%6dain.rs"), key("file:///tmp/main.rs"));
        for uri in [
            "file:///tmp/My%20Project/Ü2/a.rs",
            "file:///tmp/My%20Project/%C3%9C/../a.rs",
            "file:///tmp/My%20Project/%C3%9C/%00.rs",
            "file://remote/tmp/a.rs",
            "file:///tmp/%ZZ",
        ] {
            assert!(relative_file(root, uri).is_none(), "{uri}");
        }
    }
}
