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

//! Start the Fastxt sync server and print its pairing code.

use clap::Parser;
use std::sync::Arc;

/// Run a Fastxt sync server until Ctrl-C.
#[derive(Parser, Debug)]
#[command(name = "fastxt-server", version)]
struct Args {
    /// Port to listen on (default 3456).
    #[arg(short, long)]
    port: Option<u16>,

    /// Database file (default: the platform's Fastxt directory).
    #[arg(short, long)]
    db: Option<String>,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let args = Args::parse();
    let open = |path: Option<&str>| -> fastxt_core::Result<_> {
        match path {
            Some(p) => fastxt_core::Fastxt::open(p),
            None => fastxt_core::Fastxt::open_default(),
        }
    };
    let db = match open(args.db.as_deref()) {
        Ok(db) => Arc::new(std::sync::Mutex::new(db)),
        Err(e) => {
            eprintln!("cannot open the database: {e}");
            std::process::exit(1);
        }
    };

    let port = args.port.unwrap_or(fastxt_core::sync::DEFAULT_PORT);
    let handle = match fastxt_core::sync::server::serve(db, port) {
        Ok(handle) => handle,
        Err(e) => {
            eprintln!("cannot start the sync server: {e}");
            std::process::exit(1);
        }
    };

    println!("Fastxt sync server on {}", handle.addr());
    println!();
    println!("Pairing code (scan or type on the other device):");
    println!("  {}", handle.pairing_code);
    println!();
    println!("The code is valid until this server stops. Ctrl-C to exit.");

    let (tx, rx) = std::sync::mpsc::channel::<()>();
    ctrlc::set_handler(move || {
        let _ = tx.send(());
    })
    .expect("Ctrl-C handler");

    let _ = rx.recv(); // block until Ctrl-C
    handle.stop();
    println!("server stopped");
}
