/*
    Fastxt
    Copyright (C) 2020  Yi Wang

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU Affero General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU Affero General Public License for more details.

    You should have received a copy of the GNU Affero General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

//! Pairing codes: how one device trusts another over the network.
//!
//! The server shows a code (typically as a QR code) containing its address,
//! the fingerprint of its TLS certificate and a one-time session token:
//!
//! ```text
//! FASTXT1:192.168.1.5:3456:<32 hex chars>:<10 base32 chars>
//! ```
//!
//! The client pins the certificate fingerprint (no CA needed on a LAN) and
//! presents the token with every request; the server compares in constant
//! time. A new code is generated for each server session.

use crate::error::{Error, Result};

/// Version prefix of the pairing code format.
pub const PREFIX: &str = "FASTXT1";

/// Parsed pairing code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingInfo {
    /// Server host (IP or hostname).
    pub host: String,
    pub port: u16,
    /// First 128 bits of the server certificate's SHA-256, hex (32 chars).
    pub fingerprint: String,
    /// One-time session token (10 base32 chars).
    pub token: String,
}

/// SHA-256 of a certificate, shortened to 128 bits of hex.
#[must_use]
pub fn cert_fingerprint(cert_der: &[u8]) -> String {
    use ring::digest::{Context, SHA256};
    let mut context = Context::new(&SHA256);
    context.update(cert_der);
    let digest = context.finish();
    digest
        .as_ref()
        .iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
}

/// A fresh 10-character base32 token (A-Z, 2-7).
#[must_use]
pub fn generate_token() -> String {
    use ring::rand::{SecureRandom, SystemRandom};
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut bytes = [0u8; 10];
    if SystemRandom::new().fill(&mut bytes).is_err() {
        // Should not happen on supported platforms, but never panic: a
        // UUID's hex digits are an acceptable stand-in token.
        return uuid::Uuid::new_v4()
            .simple()
            .to_string()
            .chars()
            .take(10)
            .flat_map(|c| c.to_uppercase().next())
            .collect();
    }
    bytes
        .iter()
        .map(|b| ALPHABET[usize::from(*b) % ALPHABET.len()] as char)
        .collect()
}

/// Constant-time string comparison (token and fingerprint checks). Only the
/// lengths leak; the comparison itself takes the same path for every byte.
#[must_use]
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

impl PairingInfo {
    /// Build from parts, validating shapes.
    ///
    /// # Errors
    /// [`Error::Invalid`] on a bad host, port, fingerprint or token.
    pub fn new(
        host: impl Into<String>,
        port: u16,
        fingerprint: String,
        token: String,
    ) -> Result<Self> {
        let host = host.into();
        if host.is_empty() || host.contains(':') || host.contains(char::is_whitespace) {
            return Err(Error::Invalid(
                "pairing host must be an IP or hostname".into(),
            ));
        }
        if port == 0 {
            return Err(Error::Invalid("pairing port must not be zero".into()));
        }
        if fingerprint.len() != 32 || !fingerprint.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::Invalid(
                "pairing fingerprint must be 32 hex characters".into(),
            ));
        }
        if token.len() != 10 || !token.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(Error::Invalid("pairing token must be 10 characters".into()));
        }
        Ok(PairingInfo {
            host,
            port,
            fingerprint: fingerprint.to_lowercase(),
            token: token.to_uppercase(),
        })
    }

    /// Encode as a pairing code string.
    #[must_use]
    pub fn encode(&self) -> String {
        format!(
            "{PREFIX}:{}:{}:{}:{}",
            self.host, self.port, self.fingerprint, self.token
        )
    }

    /// Parse a pairing code; whitespace and case are forgiven.
    ///
    /// # Errors
    /// [`Error::Invalid`] when the code is not in the FASTXT1 shape.
    pub fn decode(code: &str) -> Result<Self> {
        let code = code.trim().trim_end_matches('\n');
        let parts: Vec<&str> = code.split(':').collect();
        if parts.len() != 5 || !parts[0].eq_ignore_ascii_case(PREFIX) {
            return Err(Error::Invalid(format!(
                "not a {PREFIX} pairing code: {}",
                crate::ai::truncate(code, 60)
            )));
        }
        let port: u16 = parts[2]
            .parse()
            .map_err(|_| Error::Invalid(format!("bad port in pairing code: {}", parts[2])))?;
        PairingInfo::new(parts[1], port, parts[3].to_string(), parts[4].to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PairingInfo {
        PairingInfo::new(
            "192.168.1.5",
            3456,
            "0123456789abcdef0123456789abcdef".into(),
            "abcdefghij".into(),
        )
        .unwrap()
    }

    #[test]
    fn round_trips() {
        let info = sample();
        let code = info.encode();
        assert_eq!(
            code,
            "FASTXT1:192.168.1.5:3456:0123456789abcdef0123456789abcdef:ABCDEFGHIJ"
        );
        assert_eq!(PairingInfo::decode(&code).unwrap(), info);
        // Case and surrounding whitespace are forgiven.
        assert_eq!(
            PairingInfo::decode(&format!("  {} \n", code.to_lowercase())).unwrap(),
            info
        );
    }

    #[test]
    fn rejects_malformed_codes() {
        assert!(PairingInfo::decode("").is_err());
        assert!(PairingInfo::decode("FASTXT1:192.168.1.5:3456:abc:token123").is_err());
        assert!(
            PairingInfo::decode(
                "FASTXT1:192.168.1.5:notaport:0123456789abcdef0123456789abcdef:ABCDEFGHIJ"
            )
            .is_err()
        );
        assert!(
            PairingInfo::decode(
                "OTHER1:192.168.1.5:3456:0123456789abcdef0123456789abcdef:ABCDEFGHIJ"
            )
            .is_err()
        );
    }

    #[test]
    fn token_fingerprint_helpers() {
        let token = generate_token();
        assert_eq!(token.len(), 10);
        assert!(
            token
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        );
        let fp = cert_fingerprint(b"certificate bytes");
        assert_eq!(fp.len(), 32);
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
        // Deterministic.
        assert_eq!(fp, cert_fingerprint(b"certificate bytes"));
    }

    #[test]
    fn constant_time_compare() {
        assert!(constant_time_eq("abcdefghij", "abcdefghij"));
        assert!(!constant_time_eq("abcdefghij", "abcdefghik"));
        assert!(!constant_time_eq("abcdefghij", "abcdefghi"));
    }
}
