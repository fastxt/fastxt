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

use clap::{Arg, Command};
use fastxt_core::exe::run;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();
    let matches = Command::new("fastxt-rpc-client")
        .arg(Arg::new("addr").short('a').long("addr"))
        .get_matches();
    let addr = matches
        .get_one::<String>("addr")
        .map_or("0.0.0.0:3456", std::string::String::as_str);
    eprintln!("addr: {addr}");
    run(&(r#"{"action":"client-stop-server", "addr": ""#.to_string() + addr + r#""}"#));
}
