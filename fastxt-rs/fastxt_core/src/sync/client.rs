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

//! The sync client: dial a paired server and converge.

use super::{CHUNK, FastxtSyncClient, PROTOCOL_VERSION, ctx};
use crate::error::{Error, Result};
use crate::model::{EmbeddingStamp, Stamp};
use crate::pairing::{PairingInfo, cert_fingerprint, constant_time_eq};
use crate::store::Fastxt;
use std::collections::HashMap;
use std::sync::Arc;
use tarpc::client;
use tarpc::serde_transport as transport;
use tarpc::tokio_serde::formats::Bincode;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls;
use tracing::debug;

/// What a sync transferred.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SyncReport {
    pub notes_pulled: u32,
    pub notes_pushed: u32,
    pub embeddings_pulled: u32,
    pub embeddings_pushed: u32,
}

/// Sync with the server named in a pairing code. Notes first (so both sides
/// agree on note versions), then embeddings per model. Safe to re-run.
///
/// # Errors
/// [`Error::Invalid`] for a malformed code; [`Error::Sync`] for connection,
/// pairing and protocol failures.
pub fn sync(pairing_code: &str, db: &mut Fastxt) -> Result<SyncReport> {
    let info = PairingInfo::decode(pairing_code)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::Sync(format!("could not start network runtime: {e}")))?;
    runtime.block_on(run(&info, db))
}

async fn connect(info: &PairingInfo) -> Result<FastxtSyncClient> {
    let verifier = Arc::new(PinVerifier {
        fingerprint: info.fingerprint.clone(),
    });
    // Explicit provider: no reliance on process-wide defaults, which other
    // crates in the same binary (e.g. reqwest's TLS stack) may have claimed.
    let builder = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| Error::Sync(format!("TLS versions: {e}")))?;
    let config = builder
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));
    let name = rustls::pki_types::ServerName::try_from("fastxt".to_string())
        .map_err(|e| Error::Sync(format!("internal: bad server name: {e}")))?;

    let addr = (info.host.as_str(), info.port);
    let tcp = TcpStream::connect(addr)
        .await
        .map_err(|e| Error::Sync(format!("cannot reach {addr:?}: {e}")))?;
    let tls = connector
        .connect(name, tcp)
        .await
        .map_err(|e| Error::Sync(format!("TLS failed (wrong pairing code?): {e}")))?;
    let framed = tarpc::tokio_util::codec::length_delimited::Builder::new()
        .max_frame_length(super::MAX_FRAME_BYTES)
        .new_framed(tls);
    let io = transport::new(framed, Bincode::default());
    Ok(FastxtSyncClient::new(client::Config::default(), io).spawn())
}

fn denied<T>(what: &str) -> Result<T> {
    Err(Error::Sync(format!(
        "the server rejected the pairing token ({what}); start a new server session and rescan"
    )))
}

/// `None` from the server means "bad token", not "empty".
fn unwrap_denied<T>(value: Option<T>, what: &str) -> Result<T> {
    value.ok_or(()).or_else(|_| denied(what))
}

fn newer(remote: &Stamp, local: &Stamp) -> bool {
    remote.updated_at > local.updated_at || remote.ai_updated_at > local.ai_updated_at
}

async fn run(info: &PairingInfo, db: &mut Fastxt) -> Result<SyncReport> {
    let client = connect(info).await?;
    let token = info.token.clone();

    let version = client
        .protocol_version(ctx())
        .await
        .map_err(|e| Error::Sync(format!("no answer from the server: {e}")))?;
    if version != PROTOCOL_VERSION {
        return Err(Error::Sync(format!(
            "protocol mismatch: server speaks v{version}, this device v{PROTOCOL_VERSION}; update Fastxt on both"
        )));
    }
    if !client
        .auth(ctx(), token.clone())
        .await
        .map_err(|e| Error::Sync(e.to_string()))?
    {
        return denied("auth");
    }

    // ---- notes ---------------------------------------------------------
    let remote: Vec<Stamp> = unwrap_denied(
        client
            .manifest(ctx(), token.clone())
            .await
            .map_err(|e| Error::Sync(e.to_string()))?,
        "manifest",
    )?;
    let local = db.manifest()?;

    let remote_map: HashMap<&str, &Stamp> = remote.iter().map(|s| (s.uuid4.as_str(), s)).collect();
    let local_map: HashMap<&str, &Stamp> = local.iter().map(|s| (s.uuid4.as_str(), s)).collect();

    let mut to_pull: Vec<String> = remote
        .iter()
        .filter(|r| match local_map.get(r.uuid4.as_str()) {
            Some(l) => newer(r, l),
            None => true,
        })
        .map(|s| s.uuid4.clone())
        .collect();
    let mut to_push: Vec<String> = local
        .iter()
        .filter(|l| match remote_map.get(l.uuid4.as_str()) {
            Some(r) => newer(l, r),
            None => true,
        })
        .map(|s| s.uuid4.clone())
        .collect();
    // Deterministic order (tests, retries).
    to_pull.sort();
    to_push.sort();

    let mut report = SyncReport::default();
    for chunk in to_pull.chunks(CHUNK) {
        let records = unwrap_denied(
            client
                .records(ctx(), token.clone(), chunk.to_vec())
                .await
                .map_err(|e| Error::Sync(e.to_string()))?,
            "records",
        )?;
        report.notes_pulled += db.apply_remote(&records)? as u32;
    }
    for chunk in to_push.chunks(CHUNK) {
        let records = db.records(chunk)?;
        let changed = unwrap_denied(
            client
                .send_records(ctx(), token.clone(), records)
                .await
                .map_err(|e| Error::Sync(e.to_string()))?,
            "send_records",
        )?;
        report.notes_pushed += changed;
    }
    debug!(?report, "notes exchanged");

    // ---- embeddings ------------------------------------------------------
    let mut models: Vec<String> = db
        .embedding_models()?
        .into_iter()
        .map(|m| m.model_id)
        .collect();
    let remote_models = unwrap_denied(
        client
            .embedding_models(ctx(), token.clone())
            .await
            .map_err(|e| Error::Sync(e.to_string()))?,
        "embedding_models",
    )?;
    for m in remote_models {
        if !models.contains(&m.model_id) {
            models.push(m.model_id);
        }
    }

    for model in models {
        let remote_emb: Option<Vec<EmbeddingStamp>> = client
            .embeddings_manifest(ctx(), token.clone(), model.clone())
            .await
            .map_err(|e| Error::Sync(e.to_string()))?;
        let Some(remote_emb) = remote_emb else {
            return denied("embeddings_manifest");
        };
        let local_emb = db.embedding_manifest(&model)?;
        let remote_set: HashMap<&str, &str> = remote_emb
            .iter()
            .map(|e| (e.note_uuid.as_str(), e.note_updated_at.as_str()))
            .collect();
        let local_set: HashMap<&str, &str> = local_emb
            .iter()
            .map(|e| (e.note_uuid.as_str(), e.note_updated_at.as_str()))
            .collect();

        let pull: Vec<String> = remote_emb
            .iter()
            .filter(|e| {
                local_set.get(e.note_uuid.as_str()).copied() != Some(e.note_updated_at.as_str())
            })
            .map(|e| e.note_uuid.clone())
            .collect();
        let push: Vec<String> = local_emb
            .iter()
            .filter(|e| {
                remote_set.get(e.note_uuid.as_str()).copied() != Some(e.note_updated_at.as_str())
            })
            .map(|e| e.note_uuid.clone())
            .collect();

        for chunk in pull.chunks(CHUNK) {
            let records = unwrap_denied(
                client
                    .embeddings(ctx(), token.clone(), model.clone(), chunk.to_vec())
                    .await
                    .map_err(|e| Error::Sync(e.to_string()))?,
                "embeddings",
            )?;
            report.embeddings_pulled += db.apply_remote_embeddings(&records)? as u32;
        }
        for chunk in push.chunks(CHUNK) {
            let records = db.embedding_records(&model, chunk)?;
            if records.is_empty() {
                continue;
            }
            let stored = unwrap_denied(
                client
                    .send_embeddings(ctx(), token.clone(), records)
                    .await
                    .map_err(|e| Error::Sync(e.to_string()))?,
                "send_embeddings",
            )?;
            report.embeddings_pushed += stored;
        }
    }
    debug!(?report, "embeddings exchanged");
    Ok(report)
}

/// Accepts exactly the certificate whose fingerprint is in the pairing code.
#[derive(Debug)]
struct PinVerifier {
    fingerprint: String,
}

impl rustls::client::danger::ServerCertVerifier for PinVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let seen = cert_fingerprint(end_entity.as_ref());
        if constant_time_eq(&seen, &self.fingerprint) {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::BadEncoding,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
        .map_err(|_| rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
        .map_err(|_| rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding))
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
            .to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NewNote;
    use crate::store::SharedDb;
    use crate::sync::server;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    /// A server backed by its own handle on `path`, on an ephemeral port.
    /// The pairing code it prints points at loopback.
    fn start_server(path: &std::path::Path) -> server::ServerHandle {
        let db: SharedDb = Arc::new(std::sync::Mutex::new(Fastxt::open(path).unwrap()));
        server::serve_on(
            db,
            SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .unwrap()
    }

    #[test]
    fn end_to_end_note_and_embedding_transfer_with_security() {
        let dir = tempfile::tempdir().unwrap();
        let server_path = dir.path().join("server.sqlite3");
        let mut server_side = Fastxt::open(&server_path).unwrap();
        let note = server_side
            .insert(NewNote::new("hello sync", "rust, sync"))
            .unwrap();
        server_side
            .store_embedding(&note.rowid.into(), "test-model", &[0.1, 0.2, 0.3])
            .unwrap();
        drop(server_side);

        let handle = start_server(&server_path);
        let mut b = Fastxt::open_in_memory().unwrap();
        let report = sync(&handle.pairing_code, &mut b).unwrap();
        assert_eq!(report.notes_pulled, 1);
        assert_eq!(report.embeddings_pulled, 1);
        let got = b.get(&note.uuid4.as_str().into()).unwrap().unwrap();
        assert_eq!(got.txt, "hello sync");
        assert_eq!(
            b.embedding(&got.rowid.into(), "test-model")
                .unwrap()
                .unwrap(),
            vec![0.1, 0.2, 0.3]
        );

        // A second run transfers nothing.
        let again = sync(&handle.pairing_code, &mut b).unwrap();
        assert_eq!(again, SyncReport::default());

        // A validly-shaped code with the wrong token is rejected.
        let parts: Vec<&str> = handle.pairing_code.split(':').collect();
        let bad_token = format!(
            "{}:{}:{}:{}:{}",
            parts[0], parts[1], parts[2], parts[3], "XXXXXXXXXX"
        );
        let mut c = Fastxt::open_in_memory().unwrap();
        assert!(sync(&bad_token, &mut c).is_err());

        handle.stop();
    }

    #[test]
    fn a_well_formed_but_wrong_fingerprint_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let handle = start_server(&dir.path().join("s.sqlite3"));
        let mut b = Fastxt::open_in_memory().unwrap();
        // Keep host:port and token, corrupt the certificate fingerprint.
        let parts: Vec<&str> = handle.pairing_code.split(':').collect();
        let evil = format!(
            "{}:{}:{}:{}:{}",
            parts[0], parts[1], parts[2], "ffffffffffffffffffffffffffffffff", parts[4]
        );
        let err = sync(&evil, &mut b).unwrap_err();
        assert!(err.to_string().contains("TLS"), "{err}");
        handle.stop();
    }

    #[test]
    fn malformed_codes_are_rejected_before_any_network() {
        let mut a = Fastxt::open_in_memory().unwrap();
        assert!(matches!(sync("nonsense", &mut a), Err(Error::Invalid(_))));
    }
}
