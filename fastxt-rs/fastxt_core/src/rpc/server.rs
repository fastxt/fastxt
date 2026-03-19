/*
    Fastxt
    Copyright (C) 2018-2019  Yi Wang

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

use tracing::{debug, info, warn};

use super::Fastxt;
use crate::cmd::insert;
use crate::cmd::sync::{diff_uuid4_from_server, diff_uuid4_to_server, get_note_by_uuid4};
use crate::cmd::{
    get_embedding_by_uuid4, get_embedding_model_id, get_embedding_uuid4s_by_model,
    store_embedding_by_uuid4,
};
use crate::exe::{ensure_db_initialized, get_sqlite_connection};
use crate::upgrade::get_meta_version;
use crate::Note;
use futures::future::{AbortHandle, Abortable, Aborted};
use futures::prelude::*;
use std::{io, net::SocketAddr};
use tarpc::{
    context,
    server::{self, Channel},
    tokio_serde::formats::Bincode,
};
use tokio::runtime::Runtime;

#[derive(Clone)]
struct FastxtServer {
    #[allow(dead_code)]
    client_addr: SocketAddr,
    abort_handle: AbortHandle,
}

impl Fastxt for FastxtServer {
    async fn is_version_match(self, _: context::Context, version: String) -> bool {
        let conn = get_sqlite_connection();
        ensure_db_initialized(&conn);
        version == get_meta_version(&conn)
    }

    async fn diff_uuid4_to_server(
        self,
        _: context::Context,
        candidates: Vec<String>,
    ) -> Vec<String> {
        let conn = get_sqlite_connection();
        ensure_db_initialized(&conn);
        diff_uuid4_to_server(&conn, candidates)
    }

    async fn diff_uuid4_from_server(
        self,
        _: context::Context,
        candidates: Vec<String>,
    ) -> Vec<String> {
        let conn = get_sqlite_connection();
        ensure_db_initialized(&conn);
        diff_uuid4_from_server(&conn, candidates)
    }

    async fn send_note(self, _: context::Context, note: Note) -> bool {
        let conn = get_sqlite_connection();
        ensure_db_initialized(&conn);
        debug!(?note, "upsert note");
        insert(&conn, note);
        true
    }

    async fn receive_note(self, _: context::Context, uuid4: String) -> Note {
        debug!(?uuid4, "receive note");
        let conn = get_sqlite_connection();
        ensure_db_initialized(&conn);
        get_note_by_uuid4(&conn, &uuid4)
    }

    async fn stop(self, _: context::Context) -> bool {
        self.abort_handle.abort();
        true
    }

    async fn get_embedding_model_id(self, _: context::Context) -> Option<String> {
        let conn = get_sqlite_connection();
        ensure_db_initialized(&conn);
        get_embedding_model_id(&conn)
    }

    async fn get_embedding_uuid4s(
        self,
        _: context::Context,
        model_id: String,
    ) -> Vec<String> {
        let conn = get_sqlite_connection();
        ensure_db_initialized(&conn);
        get_embedding_uuid4s_by_model(&conn, &model_id)
    }

    async fn receive_embedding(
        self,
        _: context::Context,
        uuid4: String,
    ) -> Option<(Vec<u8>, String)> {
        let conn = get_sqlite_connection();
        ensure_db_initialized(&conn);
        get_embedding_by_uuid4(&conn, &uuid4)
    }

    async fn send_embedding(
        self,
        _: context::Context,
        uuid4: String,
        embedding_bytes: Vec<u8>,
        model_id: String,
    ) -> bool {
        let conn = get_sqlite_connection();
        ensure_db_initialized(&conn);
        store_embedding_by_uuid4(&conn, &uuid4, &embedding_bytes, &model_id);
        true
    }
}

async fn start_server(addr: &SocketAddr) -> io::Result<()> {
    let (abort_handle, registration) = futures::future::AbortHandle::new_pair();
    let mut listener = tarpc::serde_transport::tcp::listen(addr, Bincode::default).await?;
    listener.config_mut().max_frame_length(usize::MAX);

    let server = listener
        .filter_map(|r| future::ready(r.ok()))
        .map(server::BaseChannel::with_defaults)
        .for_each(|channel| {
            let abort_handle = abort_handle.clone();
            async move {
                let client_addr = match channel.transport().peer_addr() {
                    Ok(addr) => addr,
                    Err(e) => {
                        warn!(error = %e, "failed to get peer address");
                        return;
                    }
                };
                let server = FastxtServer {
                    client_addr,
                    abort_handle,
                };
                tokio::spawn(
                    channel
                        .execute(server.serve())
                        .for_each(|_| async {}),
                );
            }
        });

    if let Err(Aborted) = Abortable::new(server, registration).await {
        info!("RPC server stopped");
    }
    Ok(())
}

pub fn start(addr: &str) -> Result<(), &'static str> {
    let server_addr: SocketAddr = addr.parse().map_err(|e| {
        warn!(addr, error = %e, "invalid server address");
        "invalid server address"
    })?;
    let rt = Runtime::new().map_err(|e| {
        warn!(error = %e, "failed to create tokio runtime");
        "failed to create runtime"
    })?;
    rt.block_on(async {
        let _ = start_server(&server_addr).await;
    });
    Ok(())
}

pub fn get_server_addr() -> String {
    match if_addrs::get_if_addrs() {
        Ok(addrs) => {
            for iface in addrs {
                if !iface.is_loopback() {
                    return format!("{}:3456", iface.addr.ip());
                }
            }
            String::new()
        }
        Err(e) => {
            warn!(error = %e, "failed to get network interfaces");
            String::new()
        }
    }
}
