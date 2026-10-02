//! TLS for RDP (ADR 0034), on rustls with `ring`: the session's keys are never logged (no
//! `KeyLogFile`, which `ironrdp-tls` sets), resumption is off (CredSSP doesn't allow it), and the
//! server's certificate is kept rather than checked against a CA, for the app to check it (trust
//! on first use) before any credential is sent.

use std::sync::{Arc, Mutex, PoisonError};

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt as _};
use tokio_rustls::rustls;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls::{DigitallySignedStruct, SignatureScheme};

/// A TLS stream.
pub type TlsStream<S> = tokio_rustls::client::TlsStream<S>;

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
