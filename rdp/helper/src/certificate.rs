//! The server's certificate, read for the question the app asks (ADR 0034): its fingerprint,
//! subject, key kind, and the public key CredSSP binds to.

use base64ct::{Base64Unpadded, Encoding as _};
use sha2::{Digest as _, Sha256};

/// A certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Certificate {
    /// `SHA256:` and the unpadded base64 of the DER's digest, as OpenSSH writes fingerprints.
    pub fingerprint: String,
    /// The subject (`CN=...`).
    pub subject: String,
    /// The key's kind (`RSA`, `ECDSA`, `Ed25519`, or the algorithm's id).
    pub key_type: String,
    /// The subject public key.
    pub public_key: Vec<u8>,
}

/// The fingerprint of a certificate's DER.
#[must_use]
pub fn fingerprint(der: &[u8]) -> String {
    format!(
        "SHA256:{}",
        Base64Unpadded::encode_string(&Sha256::digest(der))
    )
}

/// Reads a certificate's DER.
///
/// # Errors
///
/// Why it isn't an X.509 certificate.
pub fn read(der: &[u8]) -> Result<Certificate, String> {
    use x509_cert::der::Decode as _;
    let cert = x509_cert::Certificate::from_der(der).map_err(|error| error.to_string())?;
    let info = &cert.tbs_certificate.subject_public_key_info;
    let public_key = info
        .subject_public_key
        .as_bytes()
        .ok_or("an unaligned public key")?
        .to_owned();
    let key_type = match info.algorithm.oid.to_string().as_str() {
        "1.2.840.113549.1.1.1" => "RSA".to_owned(),
        "1.2.840.10045.2.1" => "ECDSA".to_owned(),
        "1.3.101.112" => "Ed25519".to_owned(),
        other => other.to_owned(),
    };
    Ok(Certificate {
        fingerprint: fingerprint(der),
        subject: cert.tbs_certificate.subject.to_string(),
        key_type,
        public_key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints() {
        assert_eq!(
            fingerprint(b"abc"),
            "SHA256:ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0"
        );
        assert!(read(b"not a certificate").is_err());
    }
}
