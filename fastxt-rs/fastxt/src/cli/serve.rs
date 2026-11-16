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

//! `fastxt serve`: the sync server, and its pairing code.

use fastxt_core::model::{AgentSession, SendRefusal};
use fastxt_core::sync::AgentHost;
use std::sync::Arc;

#[derive(clap::Args, Debug)]
pub struct Args {
    /// Port to listen on (default 3456).
    #[arg(short, long)]
    pub port: Option<u16>,

    /// Database file (default: the platform's Fastxt directory).
    #[arg(short, long)]
    pub db: Option<String>,

    /// Also let paired devices see the coding-agent sessions open in Orca on
    /// this computer and type notes into them, as you would. Nothing is
    /// typed into a session that is asking you a question.
    #[arg(long)]
    pub agents: bool,
}

pub fn run(args: &Args) -> Result<(), String> {
    let db = super::open_db(args.db.as_deref())?;
    let db = Arc::new(std::sync::Mutex::new(db));
    let port = args.port.unwrap_or(fastxt_core::sync::DEFAULT_PORT);
    let agents: Option<Arc<dyn AgentHost>> = if args.agents {
        Some(Arc::new(fastxt::dispatch::Dispatcher::new()))
    } else {
        None
    };
    let handle = fastxt_core::sync::server::serve_with(db, port, agents.clone())
        .map_err(|e| format!("cannot start the sync server: {e}"))?;

    println!("Fastxt sync server on {}", handle.addr());
    println!();
    println!("Pairing code (scan or type on the other device):");
    println!("  {}", handle.pairing_code);
    println!();
    if let Some(agents) = &agents {
        println!("{}", agents_line(agents.sessions()));
    }
    println!("The code is valid until this server stops. Ctrl-C to exit.");

    let (tx, rx) = std::sync::mpsc::channel::<()>();
    ctrlc::set_handler(move || {
        let _ = tx.send(());
    })
    .map_err(|e| format!("cannot wait for Ctrl-C: {e}"))?;
    let _ = rx.recv();
    handle.stop();
    println!("server stopped");
    Ok(())
}

/// What a paired device can do with the agent sessions here, now.
fn agents_line(sessions: Result<Vec<AgentSession>, SendRefusal>) -> String {
    match sessions {
        Ok(sessions) => {
            let open = match sessions.len() {
                0 => "no agent session is open in Orca yet".to_string(),
                1 => "1 agent session is open in Orca".to_string(),
                n => format!("{n} agent sessions are open in Orca"),
            };
            format!("Paired devices can type notes into the agent sessions open in Orca; {open}.")
        }
        Err(refusal) => format!(
            "Paired devices could type notes into the agent sessions open in Orca, but they can't be listed now: {refusal}."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agents_line_says_what_a_paired_device_can_do() {
        assert!(agents_line(Ok(vec![])).ends_with("no agent session is open in Orca yet."));
        assert!(agents_line(Ok(vec![AgentSession::default()])).contains("1 agent session is open"));
        assert!(
            agents_line(Ok(vec![AgentSession::default(); 3])).contains("3 agent sessions are open")
        );
        let line = agents_line(Err(SendRefusal::new(
            SendRefusal::UNAVAILABLE,
            "orca is not installed, or its command was not found",
        )));
        assert!(
            line.contains("can't be listed now: orca is not installed"),
            "{line}"
        );
    }
}
