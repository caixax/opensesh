//! VeNCrypt's TLS (ADR 0035), on rustls with `ring`, as the RDP helper's (ADR 0034): no key
//! logging, no resumption, and the server's certificate kept rather than checked against a CA,
//! for the app to decide on (trust on first use) before any password is sent.

use std::sync::{Arc, Mutex, PoisonError};

use base64ct::{Base64Unpadded, Encoding as _};
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt as _};
use tokio_rustls::rustls;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls::{DigitallySignedStruct, SignatureScheme};

/// A TLS stream.
pub type TlsStream<S> = tokio_rustls::client::TlsStream<S>;

/// A server's certificate, for the question the app asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Certificate {
    /// `SHA256:` and the unpadded base64 of the DER's digest, as OpenSSH writes fingerprints.
    pub fingerprint: String,
    /// The subject (`CN=...`).
    pub subject: String,
    /// The key's kind (`RSA`, `ECDSA`, `Ed25519`, or the algorithm's id).
    pub key_type: String,
}

impl Certificate {
    /// Reads a certificate's DER. One that doesn't parse still has its fingerprint.
    #[must_use]
    pub fn read(der: &[u8]) -> Self {
        use x509_cert::der::Decode as _;
        let fingerprint = format!(
            "SHA256:{}",
            Base64Unpadded::encode_string(&Sha256::digest(der))
        );
        match x509_cert::Certificate::from_der(der) {
            Ok(cert) => {
                let tbs = cert.tbs_certificate();
                let key_type = match tbs
                    .subject_public_key_info()
                    .algorithm
                    .oid
                    .to_string()
                    .as_str()
                {
                    "1.2.840.113549.1.1.1" => "RSA".to_owned(),
                    "1.2.840.10045.2.1" => "ECDSA".to_owned(),
                    "1.3.101.112" => "Ed25519".to_owned(),
                    other => other.to_owned(),
                };
                Self {
                    fingerprint,
                    subject: tbs.subject().to_string(),
                    key_type,
                }
            }
            Err(_) => Self {
                fingerprint,
                subject: String::new(),
                key_type: "X.509".to_owned(),
            },
        }
    }
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// Accepts the server's certificate and keeps it. The handshake's signatures are still
/// verified, so the server holds the certificate's key.
#[derive(Debug)]
struct Keep {
    certificate: Mutex<Option<Vec<u8>>>,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl ServerCertVerifier for Keep {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        *self
            .certificate
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(end_entity.to_vec());
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Upgrades `stream` to TLS with `server_name` (SNI): the stream and the server's certificate
/// (DER), not yet trusted.
///
/// # Errors
///
/// When the handshake fails or the server sent no certificate.
pub async fn upgrade<S>(stream: S, server_name: &str) -> std::io::Result<(TlsStream<S>, Vec<u8>)>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let provider = provider();
    let keep = Arc::new(Keep {
        certificate: Mutex::new(None),
        provider: Arc::clone(&provider),
    });
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(std::io::Error::other)?
        .dangerous()
        .with_custom_certificate_verifier(Arc::clone(&keep) as Arc<dyn ServerCertVerifier>)
        .with_no_client_auth();
    config.resumption = rustls::client::Resumption::disabled();
    // An IP address or a name; anything else (a bare `[::1]`) is sent as a name it can't be.
    let name = ServerName::try_from(server_name.trim_matches(['[', ']']).to_owned())
        .map_err(std::io::Error::other)?;
    let mut stream = tokio_rustls::TlsConnector::from(Arc::new(config))
        .connect(name, stream)
        .await?;
    stream.flush().await?;
    let certificate = keep
        .certificate
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
        .ok_or_else(|| std::io::Error::other("the server sent no certificate"))?;
    Ok((stream, certificate))
}
