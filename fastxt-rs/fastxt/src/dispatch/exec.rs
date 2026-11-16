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

//! Running a host's command: no shell, and a time limit.

use super::error::{Error, Result};
use super::model::Tool;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(10);

/// A command to run: the program and its arguments one by one, never a
/// shell line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
}

impl CommandSpec {
    pub(crate) fn new(tool: &Tool) -> Self {
        CommandSpec {
            program: tool.program.clone(),
            args: tool.args.clone(),
        }
    }

    pub(crate) fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub(crate) fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// The program's name, for messages.
    #[must_use]
    pub fn name(&self) -> String {
        self.program.file_name().map_or_else(
            || self.program.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    /// The command as a line to read. An argument longer than `width`
    /// characters is cut, since one of them is the whole prompt.
    #[must_use]
    pub fn line(&self, width: usize) -> String {
        let mut words = vec![quote(&self.program.to_string_lossy())];
        for arg in &self.args {
            if arg.chars().count() > width {
                let cut: String = arg.chars().take(width).collect();
                words.push(quote(&format!("{cut}…")));
            } else {
                words.push(quote(arg));
            }
        }
        words.join(" ")
    }

    fn spawn_error(&self, error: std::io::Error) -> Error {
        if error.kind() == std::io::ErrorKind::NotFound {
            Error::NotInstalled(self.name())
        } else {
            Error::Io(error)
        }
    }
}

/// A word as a POSIX shell would need it written.
fn quote(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c));
    if plain {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

/// What a finished command printed and how it ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// The exit code; `None` when a signal ended it.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    #[must_use]
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }

    /// What it said went wrong: its error output, or failing that its
    /// output, or failing that the exit code.
    #[must_use]
    pub fn complaint(&self) -> String {
        let said = if self.stderr.trim().is_empty() {
            self.stdout.trim()
        } else {
            self.stderr.trim()
        };
        if said.is_empty() {
            match self.code {
                Some(code) => format!("exit code {code}"),
                None => "ended by a signal".into(),
            }
        } else {
            said.to_string()
        }
    }
}

/// Runs the command to its end, or stops it at `timeout`.
///
/// Output goes to unnamed files, not pipes: a command that leaves a process
/// behind would keep a pipe open, and the read would never end.
pub(crate) fn run(spec: &CommandSpec, timeout: Duration) -> Result<Output> {
    let mut out = tempfile::tempfile()?;
    let mut err = tempfile::tempfile()?;
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(out.try_clone()?))
        .stderr(Stdio::from(err.try_clone()?));
    let mut child = command.spawn().map_err(|e| spec.spawn_error(e))?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Timeout {
                program: spec.name(),
                seconds: timeout.as_secs(),
            });
        }
        std::thread::sleep(POLL);
    };
    Ok(Output {
        code: status.code(),
        stdout: read_back(&mut out)?,
        stderr: read_back(&mut err)?,
    })
}

fn read_back(file: &mut File) -> Result<String> {
    let mut bytes = Vec::new();
    file.seek(SeekFrom::Start(0))?;
    file.read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(program: &str, args: &[&str]) -> CommandSpec {
        CommandSpec::new(&Tool::named(program)).args(args.iter().copied())
    }

    #[test]
    fn words_are_quoted_only_when_a_shell_would_need_it() {
        assert_eq!(quote("--enter"), "--enter");
        assert_eq!(quote("/code/api"), "/code/api");
        assert_eq!(quote(""), "''");
        assert_eq!(quote("two words"), "'two words'");
        assert_eq!(quote("it's"), "'it'\\''s'");
        assert_eq!(quote("修复 登录"), "'修复 登录'");
    }

    #[test]
    fn a_command_reads_as_one_line_with_the_prompt_cut() {
        let spec = spec(
            "orca",
            &[
                "terminal",
                "send",
                "--text=Fix the flaky login test, please",
            ],
        );
        assert_eq!(spec.line(20), "orca terminal send '--text=Fix the flaky…'");
        // Short enough, it is shown whole.
        assert!(
            spec.line(200)
                .ends_with("'--text=Fix the flaky login test, please'")
        );
    }

    #[test]
    fn a_missing_program_is_not_installed() {
        let spec = spec("fastxt-no-such-program-anywhere", &[]);
        match run(&spec, Duration::from_secs(5)) {
            Err(Error::NotInstalled(name)) => assert_eq!(name, "fastxt-no-such-program-anywhere"),
            other => panic!("expected NotInstalled, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn output_and_exit_code_come_back() {
        let spec = spec("/bin/sh", &["-c", "echo out; echo err >&2; exit 3"]);
        let output = run(&spec, Duration::from_secs(10)).expect("runs");
        assert_eq!(output.code, Some(3));
        assert_eq!(output.stdout, "out\n");
        assert_eq!(output.stderr, "err\n");
        assert!(!output.success());
        assert_eq!(output.complaint(), "err");
    }

    #[cfg(unix)]
    #[test]
    fn a_command_that_leaves_a_process_behind_still_returns() {
        let spec = spec("/bin/sh", &["-c", "sleep 20 & echo started"]);
        let started = Instant::now();
        let output = run(&spec, Duration::from_secs(15)).expect("runs");
        assert_eq!(output.stdout, "started\n");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "waited for the process left behind"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_command_that_hangs_is_stopped() {
        let spec = spec("/bin/sh", &["-c", "sleep 30"]);
        let started = Instant::now();
        match run(&spec, Duration::from_millis(200)) {
            Err(Error::Timeout { program, .. }) => assert_eq!(program, "sh"),
            other => panic!("expected Timeout, got {other:?}"),
        }
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
