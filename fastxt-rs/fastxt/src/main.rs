/*
    Fastxt
    Copyright (C) 2026  Yi Wang

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

//! The `fastxt` command.

mod cli;

use clap::{Parser, Subcommand};
use std::io::IsTerminal;

/// Local-first notes that sync between your phone and your computers with no
/// server in between, and go into the coding-agent session you pick.
#[derive(Parser, Debug)]
#[command(name = "fastxt", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Let paired devices sync with this computer until Ctrl-C; with
    /// --agents, also let them type notes into the coding-agent sessions
    /// open here.
    Serve(cli::serve::Args),
    /// Sync this computer's notes with a paired one, once.
    Sync(cli::sync::Args),
    /// List the coding-agent sessions open here, or on a paired computer.
    Sessions(cli::agents::SessionsArgs),
    /// Type a note, or text, into an open coding-agent session, as you would.
    Send(cli::agents::SendArgs),
    /// Let agents use your notes: an MCP server on stdin and stdout.
    Mcp,
}

fn main() {
    let cli = Cli::parse();
    // Logs go to stderr: stdout is the command's output, and `mcp`'s protocol.
    // A server says what it does; the other commands only warn.
    let level = match cli.command {
        Command::Serve(_) | Command::Mcp => tracing::Level::INFO,
        _ => tracing::Level::WARN,
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env().add_directive(level.into()),
        )
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();

    let result = match &cli.command {
        Command::Serve(args) => cli::serve::run(args),
        Command::Sync(args) => cli::sync::run(args),
        Command::Sessions(args) => cli::agents::sessions(args),
        Command::Send(args) => cli::agents::send(args),
        Command::Mcp => fastxt::mcp::serve().map_err(|e| format!("the MCP server stopped: {e}")),
    };
    if let Err(message) = result {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_arguments_parse_into_one_command() {
        Cli::command().debug_assert();
        let code = "FASTXT1:192.168.1.5:3456:ffffffffffffffffffffffffffffffff:AAAAAAAAAA";
        for args in [
            vec!["fastxt", "serve", "--agents", "-p", "0"],
            vec!["fastxt", "sync", code],
            vec!["fastxt", "sessions", "--json"],
            vec!["fastxt", "sessions", "--on", code],
            vec!["fastxt", "send", "--text", "-v is noisy", "--to", "5e47"],
            vec![
                "fastxt", "send", "--note", "7", "--to", "5e47", "--on", code,
            ],
            vec!["fastxt", "mcp"],
        ] {
            assert!(Cli::try_parse_from(&args).is_ok(), "{args:?}");
        }
        assert!(
            Cli::try_parse_from(["fastxt"]).is_err(),
            "a command is needed"
        );
        assert!(Cli::try_parse_from(["fastxt", "server"]).is_err());
    }
}
