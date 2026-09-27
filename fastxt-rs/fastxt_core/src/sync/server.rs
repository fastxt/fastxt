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

use super::{FastxtSync, MAX_FRAME_BYTES, PROTOCOL_VERSION};
use crate::error::{Error, Result};
use crate::pairing::{PairingInfo, cert_fingerprint, constant_time_eq, generate_token};
use crate::store::SharedDb;
use futures::future::{AbortHandle, Abortable};
use futures::prelude::*;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tarpc::serde_transport as transport;
use tarpc::server::{BaseChannel, Channel};
use tarpc::tokio_serde::formats::Bincode;
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls;
use tokio_rustls::server::TlsStream;
use tracing::{info, warn};

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
/// network (first non-loopback IPv4 interface).
#[must_use]
pub fn lan_ip() -> Option<IpAddr> {
    let addrs = if_addrs::get_if_addrs().ok()?;
    addrs
        .iter()
        .filter(|i| !i.is_loopback())
        .map(|i| i.ip())
        .find(IpAddr::is_ipv4)
}

/// Start the sync server in the background, advertised on the LAN address.
/// Binds 0.0.0.0 on `port` (0 picks a free port).
///
/// Every connection needs TLS with this session's certificate plus the
/// session token; there is no remote shutdown.
///
/// # Errors
/// Fails if no LAN address exists, the port is taken, or TLS setup fails.
pub fn serve(db: SharedDb, port: u16) -> Result<ServerHandle> {
    let ip = lan_ip().ok_or_else(|| {
        Error::Sync("no LAN address found; are you connected to a network?".into())
    })?;
    serve_on(db, SocketAddr::from(([0, 0, 0, 0], port)), ip)
}

/// Like [`serve`], but on an explicit bind address (tests, loopback setups).
/// Port 0 picks a free port; the advertised address uses the real one.
///
/// # Errors
/// See [`serve`].
pub fn serve_on(db: SharedDb, bind: SocketAddr, advertised_ip: IpAddr) -> Result<ServerHandle> {
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
                            let conn = listener.accept().await;
                            let Ok((stream, _peer)) = conn else {
                                continue;
                            };
                            let Ok(tls) = acceptor.accept(stream).await else {
                                continue;
                            };
                            spawn_channel(tls, db.clone(), token.clone());
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

fn spawn_channel(tls: TlsStream<TcpStream>, db: SharedDb, token: Arc<String>) {
    tokio::spawn(async move {
        let framed = tarpc::tokio_util::codec::length_delimited::Builder::new()
            .max_frame_length(MAX_FRAME_BYTES)
            .new_framed(tls);
        let io = transport::new(framed, Bincode::default());
        let service = SyncService { db, token };
        BaseChannel::with_defaults(io)
            .execute(service.serve())
            .for_each(|response| async {
                tokio::spawn(response);
            })
            .await;
    });
}

#[derive(Clone)]
struct SyncService {
    db: SharedDb,
    token: Arc<String>,
}

impl SyncService {
    fn authorized(&self, token: &str) -> bool {
        constant_time_eq(token, &self.token)
    }
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
}
