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
    let addr = run(r#"{"action":"server-addr"}"#);
    eprintln!("server addr: {}", addr);

    let matches = Command::new("fastxt-rpc-server")
        .arg(
            Arg::new("addr")
                .short('a')
                .long("addr"),
        )
        .get_matches();
    let addr = matches.get_one::<String>("addr").map(|s| s.as_str()).unwrap_or("0.0.0.0:3456");
    eprintln!("addr: {}", addr);
    run(&(r#"{"action":"server", "addr": ""#.to_string() + addr + r#""}"#));
}
