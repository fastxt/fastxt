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

//! The sync server: TLS listener, token auth, no remote shutdown.

use super::{AgentHost, FastxtSync, MAX_FRAME_BYTES, PROTOCOL_VERSION};
use crate::error::{Error, Result};
use crate::model::{AgentSession, SendRefusal, Sent};
use crate::pairing::{PairingInfo, cert_fingerprint, constant_time_eq, generate_token};
use crate::sent::NewSent;
use crate::store::SharedDb;
use futures::future::{AbortHandle, Abortable};
use futures::prelude::*;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tarpc::serde_transport as transport;
use tarpc::server::{BaseChannel, Channel};
use tarpc::tokio_serde::formats::Bincode;
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls;
use tokio_rustls::server::TlsStream;
use tracing::{debug, info, warn};

/// How long a peer gets to finish the TLS handshake after connecting.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// A running sync server. Stopping it (or dropping every clone) invalidates
/// the pairing code.
#[derive(Clone)]
pub struct ServerHandle {
    addr: SocketAddr,
    /// The pairing code clients scan — address, cert fingerprint, token.
    pub pairing_code: String,
    abort: AbortHandle,
}

/// The process-wide server started through the JSON API/FFI (one per app).
static GLOBAL_SERVER: std::sync::Mutex<Option<ServerHandle>> = std::sync::Mutex::new(None);

/// Start (or replace) the process-wide sync server.
///
/// # Errors
/// See [`serve`].
pub fn start_global(db: SharedDb, port: u16) -> Result<ServerHandle> {
    let handle = serve(db, port)?;
    if let Ok(mut slot) = GLOBAL_SERVER.lock() {
        if let Some(old) = slot.take() {
            old.stop();
        }
        *slot = Some(handle.clone());
    }
    Ok(handle)
}

/// Stop the process-wide sync server, if running.
pub fn stop_global() {
    if let Ok(mut slot) = GLOBAL_SERVER.lock()
        && let Some(old) = slot.take()
    {
        old.stop();
    }
}

/// The process-wide server, if one is running.
#[must_use]
pub fn global() -> Option<ServerHandle> {
    GLOBAL_SERVER.lock().ok().and_then(|slot| slot.clone())
}

impl ServerHandle {
    /// Stop the server. The pairing code is invalid afterwards.
    pub fn stop(&self) {
        self.abort.abort();
    }

    /// The advertised address (LAN IP and port).
    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

impl std::fmt::Debug for ServerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerHandle")
            .field("addr", &self.addr)
            .field("pairing_code", &self.pairing_code)
            .finish_non_exhaustive()
    }
}

/// The LAN IPv4 address most likely reachable from a phone on the same
/// network: a private (RFC 1918) address when there is one, otherwise the
/// first address that is neither loopback nor link-local.
#[must_use]
pub fn lan_ip() -> Option<IpAddr> {
    let addrs = if_addrs::get_if_addrs().ok()?;
    pick_lan_ip(addrs.iter().filter_map(|i| match i.ip() {
        IpAddr::V4(v4) => Some(v4),
        IpAddr::V6(_) => None,
    }))
    .map(IpAddr::V4)
}

/// [`lan_ip`]'s choice among candidate addresses, in interface order.
fn pick_lan_ip(candidates: impl Iterator<Item = Ipv4Addr>) -> Option<Ipv4Addr> {
    let usable: Vec<Ipv4Addr> = candidates
        .filter(|a| !a.is_loopback() && !a.is_link_local() && !a.is_unspecified())
        .collect();
    usable
        .iter()
        .find(|a| a.is_private())
        .or_else(|| usable.first())
        .copied()
}

/// Start the sync server in the background, advertised on the LAN address.
/// Binds 0.0.0.0 on `port` (0 picks a free port).
///
/// Every connection needs TLS with this session's certificate plus the
/// session token; there is no remote shutdown. It offers no agent sessions;
/// [`serve_with`] does.
///
/// # Errors
/// Fails if no LAN address exists, the port is taken, or TLS setup fails.
pub fn serve(db: SharedDb, port: u16) -> Result<ServerHandle> {
    serve_with(db, port, None)
}

/// Like [`serve`], and with an [`AgentHost`] it also lets paired devices see
/// the coding-agent sessions open on this computer and type notes into them.
///
/// # Errors
/// See [`serve`].
pub fn serve_with(
    db: SharedDb,
    port: u16,
    agents: Option<Arc<dyn AgentHost>>,
) -> Result<ServerHandle> {
    let ip = lan_ip().ok_or_else(|| {
        Error::Sync("no LAN address found; are you connected to a network?".into())
    })?;
    serve_on_with(db, SocketAddr::from(([0, 0, 0, 0], port)), ip, agents)
}

/// Like [`serve`], but on an explicit bind address (tests, loopback setups).
/// Port 0 picks a free port; the advertised address uses the real one.
///
/// # Errors
/// See [`serve`].
pub fn serve_on(db: SharedDb, bind: SocketAddr, advertised_ip: IpAddr) -> Result<ServerHandle> {
    serve_on_with(db, bind, advertised_ip, None)
}

/// [`serve_on`] with an optional [`AgentHost`], as [`serve_with`].
///
/// # Errors
/// See [`serve`].
pub fn serve_on_with(
    db: SharedDb,
    bind: SocketAddr,
    advertised_ip: IpAddr,
    agents: Option<Arc<dyn AgentHost>>,
) -> Result<ServerHandle> {
    // Bind synchronously so the real port (0 → assigned) is known now.
    let std_listener = std::net::TcpListener::bind(bind)
        .map_err(|e| Error::Sync(format!("cannot listen on {bind}: {e}")))?;
    let bound = std_listener
        .local_addr()
        .map_err(|e| Error::Sync(format!("cannot determine listen address: {e}")))?;
    std_listener
        .set_nonblocking(true)
        .map_err(|e| Error::Sync(format!("cannot set non-blocking mode: {e}")))?;

    // A fresh certificate and token for this server session.
    let certified = rcgen::generate_simple_self_signed(vec!["fastxt".to_string()])
        .map_err(|e| Error::Sync(format!("certificate generation failed: {e}")))?;
    let fingerprint = cert_fingerprint(certified.cert.der().as_ref());
    let token = generate_token();
    let key = rustls::pki_types::PrivateKeyDer::try_from(certified.key_pair.serialize_der())
        .map_err(|e| Error::Sync(format!("key serialization failed: {e}")))?;
    // Explicit provider: no reliance on process-wide defaults, which other
    // crates in the same binary (e.g. reqwest's TLS stack) may have claimed.
    let server_config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| Error::Sync(format!("TLS versions: {e}")))?
    .with_no_client_auth()
    .with_single_cert(vec![certified.cert.der().clone()], key)
    .map_err(|e| Error::Sync(format!("TLS setup failed: {e}")))?;
    let acceptor = TlsAcceptor::from(Arc::new(server_config));

    let (abort, registration) = AbortHandle::new_pair();
    let token = Arc::new(token);

    std::thread::Builder::new()
        .name("fastxt-sync-server".into())
        .spawn({
            let db = db.clone();
            let token = token.clone();
            move || {
                let runtime = match tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        warn!(error = %e, "sync server runtime failed to start");
                        return;
                    }
                };
                runtime.block_on(async move {
                    let listener = match tokio::net::TcpListener::from_std(std_listener) {
                        Ok(l) => l,
                        Err(e) => {
                            warn!(error = %e, "sync server listener handover failed");
                            return;
                        }
                    };
                    info!(%bound, "sync server listening");
                    let accept_loop = async move {
                        loop {
                            let (stream, peer) = match listener.accept().await {
                                Ok(conn) => conn,
                                Err(e) => {
                                    // Out of descriptors or a transient fault:
                                    // retry without spinning.
                                    warn!(error = %e, "accept failed");
                                    tokio::time::sleep(Duration::from_millis(250)).await;
                                    continue;
                                }
                            };
                            // The handshake runs on its own task with a
                            // deadline, so a peer that connects and goes quiet
                            // never holds up the next one.
                            let acceptor = acceptor.clone();
                            let service = SyncService {
                                db: db.clone(),
                                token: token.clone(),
                                agents: agents.clone(),
                            };
                            tokio::spawn(async move {
                                let handshake = acceptor.accept(stream);
                                match tokio::time::timeout(HANDSHAKE_TIMEOUT, handshake).await {
                                    Ok(Ok(tls)) => serve_connection(tls, service).await,
                                    Ok(Err(e)) => debug!(%peer, error = %e, "TLS handshake failed"),
                                    Err(_) => debug!(%peer, "TLS handshake timed out"),
                                }
                            });
                        }
                    };
                    let _ = Abortable::new(accept_loop, registration).await;
                    info!("sync server stopped");
                });
            }
        })
        .map_err(|e| Error::Sync(format!("could not start server thread: {e}")))?;

    let addr = SocketAddr::new(advertised_ip, bound.port());
    let pairing = PairingInfo::new(
        addr.ip().to_string(),
        addr.port(),
        fingerprint,
        token.to_string(),
    )?;
    Ok(ServerHandle {
        addr,
        pairing_code: pairing.encode(),
        abort,
    })
}

/// Answer RPCs on one established connection until the peer hangs up.
async fn serve_connection(tls: TlsStream<TcpStream>, service: SyncService) {
    let framed = tarpc::tokio_util::codec::length_delimited::Builder::new()
        .max_frame_length(MAX_FRAME_BYTES)
        .new_framed(tls);
    let io = transport::new(framed, Bincode::default());
    BaseChannel::with_defaults(io)
        .execute(service.serve())
        .for_each(|response| async {
            tokio::spawn(response);
        })
        .await;
}

#[derive(Clone)]
struct SyncService {
    db: SharedDb,
    token: Arc<String>,
    agents: Option<Arc<dyn AgentHost>>,
}

impl SyncService {
    fn authorized(&self, token: &str) -> bool {
        constant_time_eq(token, &self.token)
    }

    fn agents(&self) -> std::result::Result<Arc<dyn AgentHost>, SendRefusal> {
        self.agents.clone().ok_or_else(|| {
            SendRefusal::new(
                SendRefusal::UNAVAILABLE,
                "this computer does not offer its agent sessions to paired devices (run `fastxt serve --agents` there)",
            )
        })
    }
}

/// Runs a host's blocking call off the network thread.
async fn blocking<T, F>(what: &str, f: F) -> std::result::Result<T, SendRefusal>
where
    T: Send + 'static,
    F: FnOnce() -> std::result::Result<T, SendRefusal> + Send + 'static,
{
    tokio::task::spawn_blocking(f).await.unwrap_or_else(|e| {
        Err(SendRefusal::new(
            SendRefusal::FAILED,
            format!("{what} stopped unexpectedly: {e}"),
        ))
    })
}

impl FastxtSync for SyncService {
    async fn protocol_version(self, _: tarpc::context::Context) -> u32 {
        PROTOCOL_VERSION
    }

    async fn auth(self, _: tarpc::context::Context, token: String) -> bool {
        self.authorized(&token)
    }

    async fn manifest(
        self,
        _: tarpc::context::Context,
        token: String,
    ) -> Option<Vec<crate::model::Stamp>> {
        if !self.authorized(&token) {
            return None;
        }
        self.db.lock().ok()?.manifest().ok()
    }

    async fn records(
        self,
        _: tarpc::context::Context,
        token: String,
        uuids: Vec<String>,
    ) -> Option<Vec<crate::model::SyncNote>> {
        if !self.authorized(&token) {
            return None;
        }
        self.db.lock().ok()?.records(&uuids).ok()
    }

    async fn embedding_models(
        self,
        _: tarpc::context::Context,
        token: String,
    ) -> Option<Vec<crate::model::EmbeddingModelInfo>> {
        if !self.authorized(&token) {
            return None;
        }
        self.db.lock().ok()?.embedding_models().ok()
    }

    async fn embeddings_manifest(
        self,
        _: tarpc::context::Context,
        token: String,
        model_id: String,
    ) -> Option<Vec<crate::model::EmbeddingStamp>> {
        if !self.authorized(&token) {
            return None;
        }
        self.db.lock().ok()?.embedding_manifest(&model_id).ok()
    }

    async fn embeddings(
        self,
        _: tarpc::context::Context,
        token: String,
        model_id: String,
        uuids: Vec<String>,
    ) -> Option<Vec<crate::model::SyncEmbedding>> {
        if !self.authorized(&token) {
            return None;
        }
        self.db
            .lock()
            .ok()?
            .embedding_records(&model_id, &uuids)
            .ok()
    }

    async fn send_records(
        self,
        _: tarpc::context::Context,
        token: String,
        records: Vec<crate::model::SyncNote>,
    ) -> Option<u32> {
        if !self.authorized(&token) {
            return None;
        }
        self.db
            .lock()
            .ok()?
            .apply_remote(&records)
            .ok()
            .map(|n| n as u32)
    }

    async fn send_embeddings(
        self,
        _: tarpc::context::Context,
        token: String,
        records: Vec<crate::model::SyncEmbedding>,
    ) -> Option<u32> {
        if !self.authorized(&token) {
            return None;
        }
        self.db
            .lock()
            .ok()?
            .apply_remote_embeddings(&records)
            .ok()
            .map(|n| n as u32)
    }

    async fn sent_manifest(self, _: tarpc::context::Context, token: String) -> Option<Vec<String>> {
        if !self.authorized(&token) {
            return None;
        }
        self.db.lock().ok()?.sent_manifest().ok()
    }

    async fn sent_records(
        self,
        _: tarpc::context::Context,
        token: String,
        uuids: Vec<String>,
    ) -> Option<Vec<Sent>> {
        if !self.authorized(&token) {
            return None;
        }
        self.db.lock().ok()?.sent_records(&uuids).ok()
    }

    async fn send_sent(
        self,
        _: tarpc::context::Context,
        token: String,
        records: Vec<Sent>,
    ) -> Option<u32> {
        if !self.authorized(&token) {
            return None;
        }
        self.db
            .lock()
            .ok()?
            .apply_remote_sent(&records)
            .ok()
            .map(|n| n as u32)
    }

    async fn agent_sessions(
        self,
        _: tarpc::context::Context,
        token: String,
    ) -> Option<std::result::Result<Vec<AgentSession>, SendRefusal>> {
        if !self.authorized(&token) {
            return None;
        }
        let agents = match self.agents() {
            Ok(agents) => agents,
            Err(refusal) => return Some(Err(refusal)),
        };
        Some(blocking("listing the sessions", move || agents.sessions()).await)
    }

    async fn send_to_session(
        self,
        _: tarpc::context::Context,
        token: String,
        note_uuid: String,
        text: String,
        handle: String,
    ) -> Option<std::result::Result<Sent, SendRefusal>> {
        if !self.authorized(&token) {
            return None;
        }
        Some(self.type_note(note_uuid, text, handle).await)
    }
}

impl SyncService {
    async fn type_note(
        &self,
        note_uuid: String,
        text: String,
        handle: String,
    ) -> std::result::Result<Sent, SendRefusal> {
        let agents = self.agents()?;
        if note_uuid.trim().is_empty() {
            return Err(SendRefusal::new(
                SendRefusal::INVALID,
                "the note to send has no id",
            ));
        }
        let typed = blocking("typing the note", move || agents.send(&handle, &text)).await?;
        let sent = NewSent {
            note_uuid,
            host: typed.host,
            agent: typed.agent,
            session: typed.session,
            cwd: typed.cwd,
            started: typed.started,
        }
        .into_record();
        // Kept here, where it happened. Should that fail, the note was typed
        // all the same: the caller keeps the record, and the next sync
        // brings it back here.
        let stored = self
            .db
            .lock()
            .map_err(|_| "the database is locked".to_string())
            .and_then(|mut db| {
                db.apply_remote_sent(std::slice::from_ref(&sent))
                    .map_err(|e| e.to_string())
            });
        if let Err(e) = stored {
            warn!(error = %e, "a sent record could not be stored here");
        }
        Ok(sent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lan_ip_prefers_private_addresses_and_skips_link_local() {
        let pick = |list: &[[u8; 4]]| pick_lan_ip(list.iter().map(|o| Ipv4Addr::from(*o)));
        assert_eq!(
            pick(&[
                [127, 0, 0, 1],
                [169, 254, 1, 2],
                [203, 0, 113, 9],
                [192, 168, 1, 5]
            ]),
            Some(Ipv4Addr::new(192, 168, 1, 5))
        );
        assert_eq!(
            pick(&[[10, 0, 0, 2], [192, 168, 1, 5]]),
            Some(Ipv4Addr::new(10, 0, 0, 2)),
            "interface order decides among private addresses"
        );
        assert_eq!(
            pick(&[[127, 0, 0, 1], [203, 0, 113, 9]]),
            Some(Ipv4Addr::new(203, 0, 113, 9)),
            "a public address beats none"
        );
        assert_eq!(
            pick(&[[127, 0, 0, 1], [169, 254, 1, 2], [0, 0, 0, 0]]),
            None
        );
    }
}
