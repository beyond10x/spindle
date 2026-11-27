//! The TLS a served `run` listens with (`story:listener-tls`): `--tls-cert` and `--tls-key`, and
//! with `--tls-client-ca` a client certificate that CA signed required of every client.
//!
//! Each file is read once at start, as PEM, and the configuration is built on `ring` with
//! TLS 1.2 and 1.3, the versions an outbound call offers ([`crate::calls`]). A renewed
//! certificate is read by a restart.

use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{RootCertStore, ServerConfig};

/// The files of `--tls-cert`, `--tls-key` and `--tls-client-ca`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Files<'a> {
    pub(crate) cert: &'a Path,
    pub(crate) key: &'a Path,
    pub(crate) client_ca: Option<&'a Path>,
}

/// The listener's TLS configuration from `files`.
///
/// # Errors
/// One line naming the flag and its file: a file that cannot be read, or holds no certificate or
/// no private key; or, naming both files, a key that does not match the certificate.
pub(crate) fn server_config(files: Files<'_>) -> Result<Arc<ServerConfig>, String> {
    let Files {
        cert,
        key,
        client_ca,
    } = files;
    let chain = certificates("--tls-cert", cert)?;
    let private = PrivateKeyDer::from_pem_file(key)
        .map_err(|error| format!("--tls-key {}: {error}", key.display()))?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = ServerConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .map_err(|error| format!("choosing the TLS versions the listener serves: {error}"))?;
    let builder = match client_ca {
        Some(path) => {
            let mut roots = RootCertStore::empty();
            for certificate in certificates("--tls-client-ca", path)? {
                roots
                    .add(certificate)
                    .map_err(|error| format!("--tls-client-ca {}: {error}", path.display()))?;
            }
            let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
                Arc::new(roots),
                provider,
            )
            .build()
            .map_err(|error| format!("--tls-client-ca {}: {error}", path.display()))?;
            builder.with_client_cert_verifier(verifier)
        }
        None => builder.with_no_client_auth(),
    };
    let config = builder
        .with_single_cert(chain, private)
        .map_err(|error| match error {
            rustls::Error::InconsistentKeys(_) => format!(
                "--tls-key {} does not match the certificate of --tls-cert {}: {error}",
                key.display(),
                cert.display()
            ),
            error => format!(
                "--tls-key {} with --tls-cert {}: {error}",
                key.display(),
                cert.display()
            ),
        })?;
    Ok(Arc::new(config))
}

/// Every certificate of the PEM file `path`, given as `flag`; a file holding none is refused.
fn certificates(flag: &str, path: &Path) -> Result<Vec<CertificateDer<'static>>, String> {
    let unreadable = |error: &dyn std::fmt::Display| format!("{flag} {}: {error}", path.display());
    let chain = CertificateDer::pem_file_iter(path)
        .map_err(|error| unreadable(&error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| unreadable(&error))?;
    if chain.is_empty() {
        return Err(format!(
            "{flag} {}: the file holds no PEM certificate",
            path.display()
        ));
    }
    Ok(chain)
}
