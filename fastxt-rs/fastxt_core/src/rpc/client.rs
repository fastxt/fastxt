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

use super::FastxtClient;
use crate::cmd::sync::get_note_by_uuid4;
use crate::cmd::sync::next_uuid4_candidates;
use crate::cmd::{
    get_embedding_model_id, get_embedding_uuid4s_by_model, insert, store_embedding_by_uuid4,
};
use crate::exe::{ensure_db_initialized, get_sqlite_connection};
use crate::upgrade::get_meta_version;
use std::net::SocketAddr;
use tarpc::{client, context, tokio_serde::formats::Bincode};
use tokio::runtime::Runtime;

type RpcResult<T> = Result<T, Box<dyn std::error::Error>>;

async fn run_sync_to_server(addr: &SocketAddr) -> RpcResult<()> {
    let transport = tarpc::serde_transport::tcp::connect(addr, Bincode::default).await?;
    let client = FastxtClient::new(client::Config::default(), transport).spawn();
    let conn = get_sqlite_connection();
    ensure_db_initialized(&conn);

    // check version
    let version = get_meta_version(&conn);
    let is_version_match = client.is_version_match(context::current(), version).await?;
    debug!(is_version_match, "version check");
    if !is_version_match {
        return Err("VERSION_NOT_MATCH".into());
    }

    // diff uuid4
    let diff_uuid4 = client
        .diff_uuid4_to_server(context::current(), next_uuid4_candidates(&conn))
        .await?;
    debug!(count = diff_uuid4.len(), "diff_uuid4_to_server");

    // send one by one
    for u in diff_uuid4 {
        client
            .send_note(context::current(), get_note_by_uuid4(&conn, &u))
            .await?;
    }
    debug!("send_note done");

    Ok(())
}

async fn run_sync_from_server(addr: &SocketAddr) -> RpcResult<()> {
    let transport = tarpc::serde_transport::tcp::connect(addr, Bincode::default).await?;
    let client = FastxtClient::new(client::Config::default(), transport).spawn();
    let conn = get_sqlite_connection();
    ensure_db_initialized(&conn);

    // check version
    let version = get_meta_version(&conn);
    let is_version_match = client.is_version_match(context::current(), version).await?;
    debug!(is_version_match, "version check");
    if !is_version_match {
        return Err("VERSION_NOT_MATCH".into());
    }

    // diff uuid4
    let diff_uuid4 = client
        .diff_uuid4_from_server(context::current(), next_uuid4_candidates(&conn))
        .await?;
    debug!(count = diff_uuid4.len(), "diff_uuid4_from_server");

    // send one by one
    for u in diff_uuid4 {
        let note = client.receive_note(context::current(), u).await?;
        insert(&conn, note);
    }
    debug!("receive_note done");

    Ok(())
}

pub fn sync(addr: &str) -> Result<String, String> {
    let server_addr: SocketAddr = addr
        .parse()
        .map_err(|e| format!("server_addr {} invalid: {}", addr, e))?;

    let rt = Runtime::new()
        .map_err(|e| format!("Failed to create tokio runtime: {}", e))?;
    let mut errors = Vec::new();

    rt.block_on(async {
        if let Err(e) = run_sync_to_server(&server_addr).await {
            warn!(error = %e, "sync to server error");
            errors.push(format!("sync-to-server: {}", e));
        } else {
            info!("sync to server done");
        }

        if let Err(e) = run_sync_from_server(&server_addr).await {
            warn!(error = %e, "sync from server error");
            errors.push(format!("sync-from-server: {}", e));
        } else {
            info!("sync from server done");
        }
    });

    if errors.is_empty() {
        Ok("sync ok".to_string())
    } else {
        Err(errors.join("; "))
    }
}

async fn run_stop_server(addr: &SocketAddr) -> RpcResult<()> {
    let transport = tarpc::serde_transport::tcp::connect(addr, Bincode::default).await?;
    let client = FastxtClient::new(client::Config::default(), transport).spawn();
    let conn = get_sqlite_connection();
    ensure_db_initialized(&conn);

    // check version
    let version = get_meta_version(&conn);
    let is_version_match = client.is_version_match(context::current(), version).await?;
    debug!(is_version_match, "version check");
    if !is_version_match {
        return Err("VERSION_NOT_MATCH".into());
    }

    let is_stopped = client.stop(context::current()).await?;
    debug!(is_stopped, "stop server result");
    Ok(())
}

pub fn stop_server(addr: &str) -> Result<String, String> {
    let server_addr: SocketAddr = addr
        .parse()
        .map_err(|e| format!("server_addr {} invalid: {}", addr, e))?;
    let rt = Runtime::new()
        .map_err(|e| format!("Failed to create tokio runtime: {}", e))?;
    rt.block_on(async {
        let resp = run_stop_server(&server_addr);
        if let Err(e) = resp.await {
            warn!(error = %e, "stop_server failed");
        }
    });
    Ok("stop ok".to_string())
}

/// Sync embeddings with the server.
/// Only syncs if both client and server use the same embedding model.
async fn run_sync_embeddings(addr: &SocketAddr) -> RpcResult<()> {
    let transport = tarpc::serde_transport::tcp::connect(addr, Bincode::default).await?;
    let client = FastxtClient::new(client::Config::default(), transport).spawn();
    let conn = get_sqlite_connection();
    ensure_db_initialized(&conn);

    // Check version match
    let version = get_meta_version(&conn);
    let is_version_match = client.is_version_match(context::current(), version).await?;
    debug!(is_version_match, "version check");
    if !is_version_match {
        return Err("VERSION_NOT_MATCH".into());
    }

    // Get local embedding model ID
    let local_model_id = get_embedding_model_id(&conn);
    debug!(?local_model_id, "local embedding model");

    // Get remote embedding model ID
    let remote_model_id = client.get_embedding_model_id(context::current()).await?;
    debug!(?remote_model_id, "remote embedding model");

    // Only sync embeddings if model IDs match
    match (local_model_id, remote_model_id) {
        (Some(local_id), Some(remote_id)) if local_id == remote_id => {
            info!("model IDs match, syncing embeddings");

            // Get local embedding UUIDs
            let local_uuid4s = get_embedding_uuid4s_by_model(&conn, &local_id);

            // Get remote embedding UUIDs
            let remote_uuid4s = client
                .get_embedding_uuid4s(context::current(), remote_id.clone())
                .await?;

            // Send embeddings that remote doesn't have
            let uuid4s_to_send: Vec<String> = local_uuid4s
                .iter()
                .filter(|u| !remote_uuid4s.contains(u))
                .cloned()
                .collect();

            info!(count = uuid4s_to_send.len(), "sending embeddings to server");
            for uuid4 in &uuid4s_to_send {
                if let Some((embedding_bytes, model_id)) =
                    crate::cmd::get_embedding_by_uuid4(&conn, uuid4)
                {
                    client
                        .send_embedding(
                            context::current(),
                            uuid4.clone(),
                            embedding_bytes,
                            model_id,
                        )
                        .await?;
                }
            }
            info!(count = uuid4s_to_send.len(), "sent embeddings to server");

            // Receive embeddings that local doesn't have
            let uuid4s_to_receive: Vec<String> = remote_uuid4s
                .iter()
                .filter(|u| !local_uuid4s.contains(u))
                .cloned()
                .collect();

            info!(count = uuid4s_to_receive.len(), "receiving embeddings from server");
            for uuid4 in &uuid4s_to_receive {
                if let Some((embedding_bytes, model_id)) = client
                    .receive_embedding(context::current(), uuid4.clone())
                    .await?
                {
                    store_embedding_by_uuid4(&conn, uuid4, &embedding_bytes, &model_id);
                }
            }
            info!(count = uuid4s_to_receive.len(), "received embeddings from server");

            Ok(())
        }
        _ => {
            info!("model IDs don't match or no embeddings exist, skipping embedding sync");
            Ok(())
        }
    }
}

/// Sync embeddings between client and server.
/// Returns a status message indicating what was synced.
pub fn sync_embeddings(addr: &str) -> Result<String, String> {
    let server_addr: SocketAddr = addr
        .parse()
        .map_err(|e| format!("server_addr {} invalid: {}", addr, e))?;

    let rt = Runtime::new()
        .map_err(|e| format!("Failed to create tokio runtime: {}", e))?;
    rt.block_on(async {
        if let Err(e) = run_sync_embeddings(&server_addr).await {
            warn!(error = %e, "sync_embeddings error");
            return Err(format!("sync_embeddings error: {}", e));
        }
        info!("sync_embeddings done");
        Ok("sync_embeddings ok".to_string())
    })
}
