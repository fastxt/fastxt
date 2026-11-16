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

//! The sync client: dial a paired server and converge, or list the agent
//! sessions open on its computer and type a note into one.

use super::{CHUNK, FastxtSyncClient, PROTOCOL_VERSION, ctx};
use crate::error::{Error, Result};
use crate::model::{AgentSession, EmbeddingStamp, NoteKey, Sent, Stamp};
use crate::pairing::{PairingInfo, cert_fingerprint, constant_time_eq};
use crate::store::Fastxt;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
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
    /// Records of what was sent to agent sessions.
    #[serde(default)]
    pub sent_pulled: u32,
    #[serde(default)]
    pub sent_pushed: u32,
}

/// How long to wait for the TCP connection and, separately, the TLS
/// handshake. Without it a mistyped LAN address hangs for minutes.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Sync with the server named in a pairing code. Notes first (so both sides
/// agree on note versions), then embeddings per model. Safe to re-run.
///
/// # Errors
/// [`Error::Invalid`] for a malformed code; [`Error::Sync`] for connection,
/// pairing and protocol failures.
pub fn sync(pairing_code: &str, db: &mut Fastxt) -> Result<SyncReport> {
    let info = PairingInfo::decode(pairing_code)?;
    runtime()?.block_on(run(&info, db))
}

/// The coding-agent sessions open on the paired computer, the one that
/// changed last first.
///
/// # Errors
/// [`Error::Refused`] when that computer doesn't offer its sessions or can't
/// list them, with its reason; [`Error::Invalid`] and [`Error::Sync`] as for
/// [`sync`].
pub fn agent_sessions(pairing_code: &str) -> Result<Vec<AgentSession>> {
    let info = PairingInfo::decode(pairing_code)?;
    runtime()?.block_on(async {
        let (client, token) = open(&info).await?;
        let reply = client
            .agent_sessions(ctx(), token)
            .await
            .map_err(|e| Error::Sync(e.to_string()))?;
        unwrap_denied(reply, "agent_sessions")?.map_err(Error::Refused)
    })
}

/// Type a note into a coding-agent session on the paired computer, as its
/// user would, and keep the record of it here too. `handle` is one of the
/// sessions [`agent_sessions`] listed.
///
/// The note's text as it is on this device is what is typed.
///
/// # Errors
/// [`Error::NotFound`] for a missing note; [`Error::Refused`] when the other
/// computer typed nothing (the session closed, or is asking its user a
/// question), or when the text reached a shell instead of an agent, with
/// its reason; [`Error::Invalid`] and [`Error::Sync`] as for [`sync`].
pub fn send_note(pairing_code: &str, db: &mut Fastxt, key: &NoteKey, handle: &str) -> Result<Sent> {
    let note = db
        .get(key)?
        .ok_or_else(|| Error::NotFound(key.to_string()))?;
    let sent = send_text(pairing_code, &note.uuid4, &note.txt, handle)?;
    db.apply_remote_sent(std::slice::from_ref(&sent))?;
    Ok(sent)
}

/// [`send_note`] without the database: types a note read beforehand and
/// returns the record, for the caller to store with
/// [`Fastxt::apply_remote_sent`]. For callers that shouldn't hold the
/// database while the other computer types, which can take seconds.
///
/// # Errors
/// As [`send_note`].
pub fn send_text(pairing_code: &str, note_uuid: &str, text: &str, handle: &str) -> Result<Sent> {
    let info = PairingInfo::decode(pairing_code)?;
    runtime()?.block_on(async {
        let (client, token) = open(&info).await?;
        let reply = client
            .send_to_session(
                ctx(),
                token,
                note_uuid.to_string(),
                text.to_string(),
                handle.to_string(),
            )
            .await
            .map_err(|e| Error::Sync(e.to_string()))?;
        unwrap_denied(reply, "send_to_session")?.map_err(Error::Refused)
    })
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::Sync(format!("could not start network runtime: {e}")))
}

/// A connection that speaks this protocol and passed the token check, with
/// the token every later call carries.
async fn open(info: &PairingInfo) -> Result<(FastxtSyncClient, String)> {
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
    Ok((client, token))
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

    let (host, port) = (info.host.as_str(), info.port);
    let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| {
            Error::Sync(format!(
                "no answer from {host}:{port} after {}s; is the server running on this network?",
                CONNECT_TIMEOUT.as_secs()
            ))
        })?
        .map_err(|e| Error::Sync(format!("cannot reach {host}:{port}: {e}")))?;
    let tls = tokio::time::timeout(CONNECT_TIMEOUT, connector.connect(name, tcp))
        .await
        .map_err(|_| Error::Sync("TLS handshake timed out".into()))?
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
    let (client, token) = open(info).await?;

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

    // ---- sent records ----------------------------------------------------
    // After the notes: tombstones have cleared their notes' records on both
    // sides by now, so none of those travel.
    let remote_sent = unwrap_denied(
        client
            .sent_manifest(ctx(), token.clone())
            .await
            .map_err(|e| Error::Sync(e.to_string()))?,
        "sent_manifest",
    )?;
    let local_sent = db.sent_manifest()?;
    let (pull, push) = missing_both_ways(&remote_sent, &local_sent);
    for chunk in pull.chunks(CHUNK) {
        let records = unwrap_denied(
            client
                .sent_records(ctx(), token.clone(), chunk.to_vec())
                .await
                .map_err(|e| Error::Sync(e.to_string()))?,
            "sent_records",
        )?;
        report.sent_pulled += db.apply_remote_sent(&records)? as u32;
    }
    for chunk in push.chunks(CHUNK) {
        let records = db.sent_records(chunk)?;
        report.sent_pushed += unwrap_denied(
            client
                .send_sent(ctx(), token.clone(), records)
                .await
                .map_err(|e| Error::Sync(e.to_string()))?,
            "send_sent",
        )?;
    }
    debug!(?report, "sent records exchanged");

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

/// The ids only `remote` has and the ids only `local` has, each sorted.
fn missing_both_ways(remote: &[String], local: &[String]) -> (Vec<String>, Vec<String>) {
    let remote_set: HashSet<&str> = remote.iter().map(String::as_str).collect();
    let local_set: HashSet<&str> = local.iter().map(String::as_str).collect();
    let mut pull: Vec<String> = remote
        .iter()
        .filter(|id| !local_set.contains(id.as_str()))
        .cloned()
        .collect();
    let mut push: Vec<String> = local
        .iter()
        .filter(|id| !remote_set.contains(id.as_str()))
        .cloned()
        .collect();
    pull.sort();
    push.sort();
    (pull, push)
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
    fn a_peer_that_never_finishes_its_handshake_does_not_block_others() {
        let dir = tempfile::tempdir().unwrap();
        let handle = start_server(&dir.path().join("s.sqlite3"));
        // A raw TCP connection that never starts TLS. With the handshake on
        // the accept loop this stalled every later client.
        let stalled = std::net::TcpStream::connect(handle.addr()).unwrap();
        let mut b = Fastxt::open_in_memory().unwrap();
        sync(&handle.pairing_code, &mut b).unwrap();
        drop(stalled);
        handle.stop();
    }

    #[test]
    fn malformed_codes_are_rejected_before_any_network() {
        let mut a = Fastxt::open_in_memory().unwrap();
        assert!(matches!(sync("nonsense", &mut a), Err(Error::Invalid(_))));
        assert!(matches!(agent_sessions("nonsense"), Err(Error::Invalid(_))));
    }

    // ---- agent sessions ---------------------------------------------------

    use crate::model::SendRefusal;
    use crate::sync::{AgentHost, Typed};

    /// A stand-in for the program that hosts the agent sessions: one session
    /// open, and it writes down what it was asked to type.
    #[derive(Default)]
    struct FakeHost {
        typed: std::sync::Mutex<Vec<(String, String)>>,
        refuse: Option<SendRefusal>,
        /// A phone's database, to see whether it is free while this types.
        phone: Option<SharedDb>,
        phone_free_while_typing: std::sync::Mutex<Option<bool>>,
    }

    impl AgentHost for FakeHost {
        fn sessions(&self) -> std::result::Result<Vec<AgentSession>, SendRefusal> {
            Ok(vec![AgentSession {
                host: "fake".into(),
                handle: "term_aaaa".into(),
                agent: "claude".into(),
                name: "Fix login".into(),
                cwd: "/code/api".into(),
                branch: Some("main".into()),
                state: "ready".into(),
                last_prompt: Some("Fix the flaky login test".into()),
                updated_at: Some(1),
            }])
        }

        fn send(&self, handle: &str, text: &str) -> std::result::Result<Typed, SendRefusal> {
            if let Some(refusal) = &self.refuse {
                return Err(refusal.clone());
            }
            if let Some(phone) = &self.phone {
                *self.phone_free_while_typing.lock().unwrap() = Some(phone.try_lock().is_ok());
            }
            self.typed
                .lock()
                .unwrap()
                .push((handle.to_string(), text.to_string()));
            Ok(Typed {
                host: "fake".into(),
                agent: "claude".into(),
                session: "Fix login".into(),
                cwd: "/code/api".into(),
                started: true,
            })
        }
    }

    fn start_server_with(path: &std::path::Path, host: Arc<FakeHost>) -> server::ServerHandle {
        let db: SharedDb = Arc::new(std::sync::Mutex::new(Fastxt::open(path).unwrap()));
        let host: Arc<dyn AgentHost> = host;
        server::serve_on_with(
            db,
            SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            Some(host),
        )
        .unwrap()
    }

    fn refused(result: Result<impl std::fmt::Debug>) -> SendRefusal {
        match result {
            Err(Error::Refused(refusal)) => refusal,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_paired_device_lists_the_sessions_and_types_a_note_into_one() {
        let dir = tempfile::tempdir().unwrap();
        let server_path = dir.path().join("desk.sqlite3");
        let host = Arc::new(FakeHost::default());
        let handle = start_server_with(&server_path, host.clone());
        let mut phone = Fastxt::open_in_memory().unwrap();
        let note = phone
            .insert(NewNote::new("Fix the flaky\nlogin test, 请", "prompt"))
            .unwrap();

        let sessions = agent_sessions(&handle.pairing_code).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].handle, "term_aaaa");
        assert_eq!(sessions[0].state, "ready");

        let key = NoteKey::Uuid(note.uuid4.clone());
        let sent = send_note(&handle.pairing_code, &mut phone, &key, "term_aaaa").unwrap();
        assert_eq!(
            *host.typed.lock().unwrap(),
            [(
                "term_aaaa".to_string(),
                "Fix the flaky\nlogin test, 请".to_string()
            )],
            "the note's text, as it is on the phone"
        );
        assert_eq!(sent.note_uuid, note.uuid4);
        assert_eq!(
            (sent.host.as_str(), sent.session.as_str()),
            ("fake", "Fix login")
        );
        assert!(sent.started);
        // Kept on both sides.
        assert_eq!(
            phone.sent(Some(&note.uuid4), 10).unwrap(),
            std::slice::from_ref(&sent)
        );
        let desk = Fastxt::open(&server_path).unwrap();
        assert_eq!(desk.sent(None, 10).unwrap(), [sent]);
        drop(desk);

        // A sync brings the note over; the record is on both sides already.
        let report = sync(&handle.pairing_code, &mut phone).unwrap();
        assert_eq!(report.notes_pushed, 1);
        assert_eq!((report.sent_pulled, report.sent_pushed), (0, 0));
        handle.stop();
    }

    #[test]
    fn the_phone_protocol_lists_and_sends_without_holding_its_database() {
        use serde_json::{Value, json};
        let dir = tempfile::tempdir().unwrap();
        let phone: SharedDb = Arc::new(std::sync::Mutex::new(Fastxt::open_in_memory().unwrap()));
        let host = Arc::new(FakeHost {
            phone: Some(phone.clone()),
            ..FakeHost::default()
        });
        let handle = start_server_with(&dir.path().join("desk.sqlite3"), host.clone());
        let call = |cmd: Value| -> Value {
            serde_json::from_str(&crate::json::run(&phone, &cmd.to_string())).unwrap()
        };
        let note = call(json!({"action": "insert", "txt": "Fix the flaky login test"}));
        let rowid = note["note"]["rowid"].clone();

        let listed = call(json!({"action": "agent-sessions", "code": handle.pairing_code}));
        assert_eq!(listed["sessions"][0]["handle"], "term_aaaa", "{listed}");
        let sent = call(json!({
            "action": "send-to-session", "code": handle.pairing_code,
            "rowid": rowid, "handle": "term_aaaa",
        }));
        assert_eq!(sent["sent"]["session"], "Fix login", "{sent}");
        assert_eq!(
            *host.phone_free_while_typing.lock().unwrap(),
            Some(true),
            "the phone's other commands could run while it typed"
        );
        let kept = call(json!({"action": "sent", "rowid": rowid}));
        assert_eq!(kept["sent"][0]["uuid4"], sent["sent"]["uuid4"]);
        // A note that isn't there is said so, before any network.
        let missing = call(json!({
            "action": "send-to-session", "code": handle.pairing_code,
            "rowid": 999, "handle": "term_aaaa",
        }));
        assert!(
            missing["error"].as_str().unwrap().contains("999"),
            "{missing}"
        );
        handle.stop();
    }

    #[test]
    fn a_server_without_agent_sessions_says_so_and_types_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let handle = start_server(&dir.path().join("desk.sqlite3"));
        let refusal = refused(agent_sessions(&handle.pairing_code));
        assert_eq!(refusal.reason, SendRefusal::UNAVAILABLE);
        assert!(refusal.message.contains("--agents"), "{refusal}");

        let mut phone = Fastxt::open_in_memory().unwrap();
        let note = phone.insert(NewNote::new("p", "")).unwrap();
        let key = NoteKey::Rowid(note.rowid);
        let refusal = refused(send_note(&handle.pairing_code, &mut phone, &key, "term_a"));
        assert_eq!(refusal.reason, SendRefusal::UNAVAILABLE);
        assert!(phone.sent(None, 10).unwrap().is_empty());
        handle.stop();
    }

    #[test]
    fn a_session_asking_a_question_gets_nothing_and_nothing_is_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let server_path = dir.path().join("desk.sqlite3");
        let host = Arc::new(FakeHost {
            refuse: Some(SendRefusal::new(
                SendRefusal::ASKING,
                "session aaaa is asking a question; answer it there, then send again",
            )),
            ..FakeHost::default()
        });
        let handle = start_server_with(&server_path, host);
        let mut phone = Fastxt::open_in_memory().unwrap();
        let note = phone.insert(NewNote::new("yes", "")).unwrap();

        let refusal = refused(send_note(
            &handle.pairing_code,
            &mut phone,
            &NoteKey::Rowid(note.rowid),
            "term_aaaa",
        ));
        assert_eq!(refusal.reason, SendRefusal::ASKING);
        assert!(refusal.message.contains("asking a question"));
        assert!(phone.sent(None, 10).unwrap().is_empty());
        assert!(
            Fastxt::open(&server_path)
                .unwrap()
                .sent(None, 10)
                .unwrap()
                .is_empty()
        );
        handle.stop();
    }

    #[test]
    fn a_missing_note_is_not_sent_and_needs_no_network() {
        let mut phone = Fastxt::open_in_memory().unwrap();
        // The code points nowhere: a lookup that reached the network would
        // fail differently.
        let code = "FASTXT1:127.0.0.1:9:ffffffffffffffffffffffffffffffff:AAAAAAAAAA";
        assert!(matches!(
            send_note(code, &mut phone, &NoteKey::Rowid(42), "term_a"),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn a_wrong_token_can_neither_list_nor_send() {
        let dir = tempfile::tempdir().unwrap();
        let host = Arc::new(FakeHost::default());
        let handle = start_server_with(&dir.path().join("desk.sqlite3"), host.clone());
        let parts: Vec<&str> = handle.pairing_code.split(':').collect();
        let bad = format!(
            "{}:{}:{}:{}:{}",
            parts[0], parts[1], parts[2], parts[3], "XXXXXXXXXX"
        );
        let err = agent_sessions(&bad).unwrap_err();
        assert!(err.to_string().contains("pairing token"), "{err}");
        let mut phone = Fastxt::open_in_memory().unwrap();
        let note = phone.insert(NewNote::new("p", "")).unwrap();
        assert!(send_note(&bad, &mut phone, &NoteKey::Rowid(note.rowid), "term_aaaa").is_err());
        assert!(host.typed.lock().unwrap().is_empty(), "nothing was typed");
        handle.stop();
    }

    #[test]
    fn sent_records_sync_both_ways_and_go_with_their_note() {
        use crate::sent::NewSent;
        let dir = tempfile::tempdir().unwrap();
        let server_path = dir.path().join("desk.sqlite3");
        let record = |note: &str, session: &str| NewSent {
            note_uuid: note.to_string(),
            host: "orca".into(),
            agent: "claude".into(),
            session: session.to_string(),
            cwd: "/code/api".into(),
            started: true,
        };
        let mut desk = Fastxt::open(&server_path).unwrap();
        let desk_note = desk.insert(NewNote::new("from the desk", "")).unwrap();
        desk.record_sent(record(&desk_note.uuid4, "desk session"))
            .unwrap();
        drop(desk);
        let handle = start_server(&server_path);

        let mut phone = Fastxt::open_in_memory().unwrap();
        let phone_note = phone.insert(NewNote::new("from the phone", "")).unwrap();
        phone
            .record_sent(record(&phone_note.uuid4, "phone session"))
            .unwrap();

        let report = sync(&handle.pairing_code, &mut phone).unwrap();
        assert_eq!((report.sent_pulled, report.sent_pushed), (1, 1));
        assert_eq!(phone.sent(None, 10).unwrap().len(), 2);
        let again = sync(&handle.pairing_code, &mut phone).unwrap();
        assert_eq!(again, SyncReport::default(), "nothing left to move");

        // Deleted on the phone: the tombstone takes the desk's copy too.
        phone
            .delete(&NoteKey::Uuid(phone_note.uuid4.clone()))
            .unwrap();
        sync(&handle.pairing_code, &mut phone).unwrap();
        let desk = Fastxt::open(&server_path).unwrap();
        let left: Vec<String> = desk
            .sent(None, 10)
            .unwrap()
            .into_iter()
            .map(|s| s.session)
            .collect();
        assert_eq!(left, ["desk session"]);
        handle.stop();
    }

    #[test]
    fn ids_missing_on_either_side_are_found_and_sorted() {
        let ids = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let (pull, push) = missing_both_ways(&ids(&["c", "a", "b"]), &ids(&["b", "e", "d"]));
        assert_eq!(pull, ids(&["a", "c"]));
        assert_eq!(push, ids(&["d", "e"]));
        let (pull, push) = missing_both_ways(&[], &[]);
        assert!(pull.is_empty() && push.is_empty());
    }
}
