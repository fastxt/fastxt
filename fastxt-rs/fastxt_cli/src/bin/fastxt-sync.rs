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

//! Sync this device with a paired Fastxt server.

use clap::Parser;

/// Sync once with the server named in a pairing code.
#[derive(Parser, Debug)]
#[command(name = "fastxt-sync", version)]
struct Args {
    /// Pairing code shown by the server, e.g. FASTXT1:192.168.1.5:3456:…:…
    code: String,

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
    let mut db = match match args.db.as_deref() {
        Some(p) => fastxt_core::Fastxt::open(p),
        None => fastxt_core::Fastxt::open_default(),
    } {
        Ok(db) => db,
        Err(e) => {
            eprintln!("cannot open the database: {e}");
            std::process::exit(1);
        }
    };

    match fastxt_core::sync::client::sync(&args.code, &mut db) {
        Ok(report) => {
            println!(
                "synced: {} notes pulled, {} pushed, {} embeddings pulled, {} pushed",
                report.notes_pulled,
                report.notes_pushed,
                report.embeddings_pulled,
                report.embeddings_pushed
            );
        }
        Err(e) => {
            eprintln!("sync failed: {e}");
            std::process::exit(1);
        }
    }
}
