//! Deterministic Chromium extension ID from the manifest `key` (SPKI DER, base64).

use sha2::{Digest, Sha256};

/// Base64 SPKI public key in `extensions/browser/manifest.json` (`key` field).
pub const PACKAGED_EXTENSION_PUBLIC_KEY_B64: &str = "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAzoapZGw2eQFZ8vBUz2PAOvpbLFX/m494GBPw/rBpAiCI9iyQGZLxjvNm42+FFTP+vqSW/iXYAF6iruyQqmMEykC6NBKTL8oCVz9dRwyDS1EQxQHO9IkGFQmQe8FB2Q1n6hyR2LICuwoDlwm1uTmljDTTPfXPMnWKvHRRlc8h7wrdJx85Ijn7PLOU7HaI1hFO8GhWyXlohY92MoRwo5JdPmEdSLNe2xJjbnPjYaX3bJe/obr/+rDoJKvpiJt/R2y0M5VcI218DXzv8OKWkSME2hSml/eHCdZKMc1gIH7GE7jklLTcIDY+xjt0jX2gP5kiVxmyqpDl8O4dKUTPn0f3oQIDAQAB";

/// Chromium extension ID for the keyed shipped manifest (`chrome-extension://…/`).
pub const PACKAGED_CHROME_EXTENSION_ID: &str = "njgbdpacpmohafoefmcmnmhdjpjmjipe";

const EXT_NAME: &str = "Jyroxa Download Integration";

pub fn chrome_extension_id_from_spki_der(der: &[u8]) -> String {
    let hash = Sha256::digest(der);
    hash.iter()
        .take(16)
        .flat_map(|b| [b >> 4, b & 0x0f])
        .map(|n| char::from(b'a' + n))
        .collect()
}

pub fn chrome_extension_id_from_manifest_key_b64(b64: &str) -> Result<String, String> {
    use base64::Engine;
    let der = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| e.to_string())?;
    Ok(chrome_extension_id_from_spki_der(&der))
}

pub fn packaged_chrome_extension_id() -> &'static str {
    PACKAGED_CHROME_EXTENSION_ID
}

pub fn extension_display_name() -> &'static str {
    EXT_NAME
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_key_matches_constant_id() {
        let id =
            chrome_extension_id_from_manifest_key_b64(PACKAGED_EXTENSION_PUBLIC_KEY_B64).unwrap();
        assert_eq!(id, PACKAGED_CHROME_EXTENSION_ID);
    }
}