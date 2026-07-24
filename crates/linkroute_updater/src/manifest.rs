use ed25519_dalek::{Signature, Verifier, VerifyingKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateManifest {
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub signature: String,
}

impl UpdateManifest {
    pub fn parse(text: &str) -> Result<Self, String> {
        let version = value_for(text, "version")?;
        let url = value_for(text, "url")?;
        let sha256 = value_for(text, "sha256")?;
        let signature = value_for(text, "signature")?;

        if sha256.len() != 64 || !sha256.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err("sha256 must be a 64 character hex string".to_string());
        }
        if signature.len() != 128 || !signature.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err("signature must be a 128 character Ed25519 hex string".to_string());
        }
        if !url.starts_with("https://") {
            return Err("url must use https".to_string());
        }

        Ok(Self { version, url, sha256, signature })
    }

    pub fn signed_payload(&self) -> String {
        format!("version={}\nurl={}\nsha256={}\n", self.version, self.url, self.sha256)
    }

    pub fn verify_signature(&self, public_key_hex: &str) -> Result<(), String> {
        verify_ed25519_hex(public_key_hex, self.signed_payload().as_bytes(), &self.signature)
    }
}

pub fn verify_ed25519_hex(
    public_key_hex: &str,
    message: &[u8],
    signature_hex: &str,
) -> Result<(), String> {
    let public_key = hex_to_array::<32>(public_key_hex)
        .map_err(|error| format!("invalid public key: {error}"))?;
    let signature =
        hex_to_array::<64>(signature_hex).map_err(|error| format!("invalid signature: {error}"))?;
    let verifying_key = VerifyingKey::from_bytes(&public_key)
        .map_err(|error| format!("invalid public key: {error}"))?;
    let signature = Signature::from_bytes(&signature);

    verifying_key
        .verify(message, &signature)
        .map_err(|error| format!("signature verification failed: {error}"))
}

fn hex_to_array<const N: usize>(hex: &str) -> Result<[u8; N], String> {
    let hex = hex.trim();
    if hex.len() != N * 2 {
        return Err(format!("expected {} hex characters", N * 2));
    }

    let mut bytes = [0_u8; N];
    for (index, byte) in bytes.iter_mut().enumerate() {
        let offset = index * 2;
        let high = hex_nibble(hex.as_bytes()[offset])?;
        let low = hex_nibble(hex.as_bytes()[offset + 1])?;
        *byte = (high << 4) | low;
    }
    Ok(bytes)
}

fn hex_nibble(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!("invalid hex byte 0x{byte:02x}")),
    }
}

fn value_for(text: &str, key: &str) -> Result<String, String> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((left, right)) = line.split_once('=') else {
            return Err(format!("invalid line: {line}"));
        };
        if left.trim() == key {
            let value = right.trim().trim_matches('"').to_string();
            if value.is_empty() {
                return Err(format!("{key} is empty"));
            }
            return Ok(value);
        }
    }
    Err(format!("missing {key}"))
}

#[cfg(test)]
mod tests {
    use super::{verify_ed25519_hex, UpdateManifest};

    const ZERO_SHA256: &str = "0000000000000000000000000000000000000000000000000000000000000000";
    const ZERO_SIGNATURE: &str = "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";

    #[test]
    fn parses_manifest() {
        let manifest = UpdateManifest::parse(&format!(
            r#"
version = "0.1.0"
url = "https://github.com/example/linkroute/releases/download/v0.1.0/LinkRouteSetup-x64.exe"
sha256 = "{ZERO_SHA256}"
signature = "{ZERO_SIGNATURE}"
"#
        ))
        .unwrap();

        assert_eq!(manifest.version, "0.1.0");
        assert_eq!(
            manifest.signed_payload(),
            "version=0.1.0\nurl=https://github.com/example/linkroute/releases/download/v0.1.0/LinkRouteSetup-x64.exe\nsha256=0000000000000000000000000000000000000000000000000000000000000000\n"
        );
    }

    #[test]
    fn rejects_http_url() {
        let result = UpdateManifest::parse(&format!(
            r#"
version = "0.1.0"
url = "http://example.invalid/file.exe"
sha256 = "{ZERO_SHA256}"
signature = "{ZERO_SIGNATURE}"
"#
        ));

        assert!(result.is_err());
    }

    #[test]
    fn rejects_non_hex_signature() {
        let result = UpdateManifest::parse(
            r#"
version = "0.1.0"
url = "https://example.invalid/file.exe"
sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
signature = "placeholder"
"#,
        );

        assert!(result.is_err());
    }

    #[test]
    fn verifies_rfc8032_test_vector() {
        let public_key = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
        let signature = "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b";

        assert!(verify_ed25519_hex(public_key, b"", signature).is_ok());
    }

    #[test]
    fn rejects_bad_signature() {
        let public_key = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";

        assert!(verify_ed25519_hex(public_key, b"changed", ZERO_SIGNATURE).is_err());
    }
}
