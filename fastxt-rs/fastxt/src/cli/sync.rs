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

//! `fastxt sync`: sync once with a paired computer.

use fastxt_core::sync::client::{self, SyncReport};

#[derive(clap::Args, Debug)]
pub struct Args {
    /// The pairing code the other computer's `fastxt serve` shows, e.g.
    /// FASTXT1:192.168.1.5:3456:…:…
    pub code: String,

    /// Print JSON.
    #[arg(long)]
    pub json: bool,

    /// Database file (default: the platform's Fastxt directory).
    #[arg(short, long)]
    pub db: Option<String>,
}

pub fn run(args: &Args) -> Result<(), String> {
    let mut db = super::open_db(args.db.as_deref())?;
    let report = client::sync(&args.code, &mut db).map_err(|e| format!("sync failed: {e}"))?;
    if args.json {
        return super::print_json(&report);
    }
    println!("{}", report_line(&report));
    Ok(())
}

fn report_line(report: &SyncReport) -> String {
    format!(
        "synced: {} notes pulled, {} pushed, {} embeddings pulled, {} pushed, {} sent records pulled, {} pushed",
        report.notes_pulled,
        report.notes_pushed,
        report.embeddings_pulled,
        report.embeddings_pushed,
        report.sent_pulled,
        report.sent_pushed
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_reads_as_one_line() {
        let report = SyncReport {
            notes_pulled: 2,
            notes_pushed: 1,
            sent_pulled: 3,
            ..SyncReport::default()
        };
        assert_eq!(
            report_line(&report),
            "synced: 2 notes pulled, 1 pushed, 0 embeddings pulled, 0 pushed, 3 sent records pulled, 0 pushed"
        );
    }
}
