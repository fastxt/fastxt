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

use super::FastxtClient;
use crate::cmd::sync::get_note_by_uuid4;
use crate::cmd::sync::next_uuid4_candidates;
use crate::cmd::{
    get_embedding_model_id, get_embedding_uuid4s_by_model, insert, store_embedding_by_uuid4,
};
use crate::exe::get_sqlite_connection;
use crate::upgrade::get_meta_version;
use std::io::Error;
use std::{io, net::SocketAddr};
use tarpc::{client, context, tokio_serde::formats::Bincode};
use tokio::runtime::Runtime;

async fn run_sync_to_server(addr: &SocketAddr) -> io::Result<()> {
    let transport = tarpc::serde_transport::tcp::connect(addr, Bincode::default).await?;
    let client = FastxtClient::new(client::Config::default(), transport).spawn();
    let conn = get_sqlite_connection();

    // check version
    let version = get_meta_version(&conn);
    let is_version_match = client.is_version_match(context::current(), version).await?;
    eprintln!("is_version_match: {}", is_version_match);
    if !is_version_match {
        return Err(Error::other("VERSION_NOT_MATCH"));
    }

    // diff uuid4
    let diff_uuid4 = client
        .diff_uuid4_to_server(context::current(), next_uuid4_candidates(&conn))
        .await?;
    eprintln!("diff_uuid4_to_server len: {:?}", diff_uuid4.len());

    // send one by one
    for u in diff_uuid4 {
        client
            .send_note(context::current(), get_note_by_uuid4(&conn, &u))
            .await?;
    }
    eprintln!("send_note done");

    Ok(())
}

async fn run_sync_from_server(addr: &SocketAddr) -> io::Result<()> {
    let transport = tarpc::serde_transport::tcp::connect(addr, Bincode::default).await?;
    let client = FastxtClient::new(client::Config::default(), transport).spawn();
    let conn = get_sqlite_connection();

    // check version
    let version = get_meta_version(&conn);
    let is_version_match = client.is_version_match(context::current(), version).await?;
    eprintln!("is_version_match: {}", is_version_match);
    if !is_version_match {
        return Err(Error::other("VERSION_NOT_MATCH"));
    }

    // diff uuid4
    let diff_uuid4 = client
        .diff_uuid4_from_server(context::current(), next_uuid4_candidates(&conn))
        .await?;
    eprintln!("diff_uuid4_from_server len: {:?}", diff_uuid4.len());

    // send one by one
    for u in diff_uuid4 {
        let note = client.receive_note(context::current(), u).await?;
        insert(&conn, note);
    }
    eprintln!("receive_note done");

    Ok(())
}

pub fn sync(addr: &str) -> Result<String, String> {
    match addr.parse() {
        Ok(server_addr) => {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                let _ = run_sync_to_server(&server_addr).await;
                eprintln!("sync to server done");
            });
            let rt2 = Runtime::new().unwrap();
            rt2.block_on(async {
                let _ = run_sync_from_server(&server_addr).await;
                eprintln!("sync from server done");
            });
            Ok("sync ok".to_string())
        }
        Err(e) => Ok(format!(r#"server_addr {} invalid: {}"#, addr, e).to_string()),
    }
}

async fn run_stop_server(addr: &SocketAddr) -> io::Result<()> {
    let transport = tarpc::serde_transport::tcp::connect(addr, Bincode::default).await?;
    let client = FastxtClient::new(client::Config::default(), transport).spawn();
    let conn = get_sqlite_connection();

    // check version
    let version = get_meta_version(&conn);
    let is_version_match = client.is_version_match(context::current(), version).await?;
    eprintln!("is_version_match: {}", is_version_match);
    if !is_version_match {
        return Err(Error::other("VERSION_NOT_MATCH"));
    }

    // diff uuid4
    let is_stopped = client.stop(context::current()).await?;
    eprintln!("is_stopped: {}", is_stopped);
    Ok(())
}

pub fn stop_server(addr: &str) -> Result<String, String> {
    let server_addr: SocketAddr = addr
        .parse()
        .unwrap_or_else(|e| panic!(r#"server_addr {} invalid: {}"#, addr, e));
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let resp = run_stop_server(&server_addr);
        if let Err(e) = resp.await {
            eprintln!("stop_server: {}.", e);
        }
    });
    Ok("stop ok".to_string())
}

/// Sync embeddings with the server.
/// Only syncs if both client and server use the same embedding model.
async fn run_sync_embeddings(addr: &SocketAddr) -> io::Result<()> {
    let transport = tarpc::serde_transport::tcp::connect(addr, Bincode::default).await?;
    let client = FastxtClient::new(client::Config::default(), transport).spawn();
    let conn = get_sqlite_connection();

    // Check version match
    let version = get_meta_version(&conn);
    let is_version_match = client.is_version_match(context::current(), version).await?;
    eprintln!("is_version_match: {}", is_version_match);
    if !is_version_match {
        return Err(Error::other("VERSION_NOT_MATCH"));
    }

    // Get local embedding model ID
    let local_model_id = get_embedding_model_id(&conn);
    eprintln!("local_model_id: {:?}", local_model_id);

    // Get remote embedding model ID
    let remote_model_id = client.get_embedding_model_id(context::current()).await?;
    eprintln!("remote_model_id: {:?}", remote_model_id);

    // Only sync embeddings if model IDs match
    match (local_model_id, remote_model_id) {
        (Some(local_id), Some(remote_id)) if local_id == remote_id => {
            eprintln!("Model IDs match, syncing embeddings...");

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

            eprintln!("Sending {} embeddings to server...", uuid4s_to_send.len());
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
            eprintln!("Sent {} embeddings to server", uuid4s_to_send.len());

            // Receive embeddings that local doesn't have
            let uuid4s_to_receive: Vec<String> = remote_uuid4s
                .iter()
                .filter(|u| !local_uuid4s.contains(u))
                .cloned()
                .collect();

            eprintln!(
                "Receiving {} embeddings from server...",
                uuid4s_to_receive.len()
            );
            for uuid4 in &uuid4s_to_receive {
                if let Some((embedding_bytes, model_id)) = client
                    .receive_embedding(context::current(), uuid4.clone())
                    .await?
                {
                    store_embedding_by_uuid4(&conn, uuid4, &embedding_bytes, &model_id);
                }
            }
            eprintln!(
                "Received {} embeddings from server",
                uuid4s_to_receive.len()
            );

            Ok(())
        }
        _ => {
            eprintln!("Model IDs don't match or no embeddings exist, skipping embedding sync");
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

    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        if let Err(e) = run_sync_embeddings(&server_addr).await {
            eprintln!("sync_embeddings error: {}", e);
            return Err(format!("sync_embeddings error: {}", e));
        }
        eprintln!("sync_embeddings done");
        Ok("sync_embeddings ok".to_string())
    })
}
