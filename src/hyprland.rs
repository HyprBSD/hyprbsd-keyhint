//! Hyprland's request socket (config errors, hyprbsd_shortcuts(), windows,
//! monitors) and its event socket.

use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::observe::{self, Baseline};
use crate::place;
use crate::shortcuts::{self, Shortcut};

const TIMEOUT: Duration = Duration::from_millis(500);
const REPLY_LIMIT: u64 = 1 << 20;
const SHORTCUTS: &str = "/repl return hyprbsd_shortcuts and hyprbsd_shortcuts()";

/// The session's bindings that run a command; None while the config has errors.
pub fn shortcuts() -> Option<Vec<Shortcut>> {
    if has_errors(&request("j/configerrors").ok()?)? {
        return None;
    }
    shortcuts::parse(&request(SHORTCUTS).ok()?)
}

/// Whether hyprctl's JSON list of config errors has one; None when it isn't such a list.
fn has_errors(reply: &str) -> Option<bool> {
    let errors: Vec<String> = serde_json::from_str(reply).ok()?;
    Some(errors.iter().any(|error| !error.trim().is_empty()))
}

/// The windows now, with their classes, the one in focus and the focused
/// monitor's name.
pub fn baseline() -> Option<(Baseline, Option<String>)> {
    let before = observe::baseline(&request("j/clients").ok()?, &request("j/activewindow").ok()?)?;
    Some((before, request("j/monitors").ok().and_then(|monitors| place::focused_monitor(&monitors))))
}

/// The `j/clients` and `j/monitors` replies, or None when either request fails.
pub fn windows_and_monitors() -> Option<(String, String)> {
    Some((request("j/clients").ok()?, request("j/monitors").ok()?))
}

/// Hyprland's event socket, read a line at a time.
pub struct Events {
    stream: UnixStream,
    pending: Vec<u8>,
}

impl Events {
    pub fn connect() -> io::Result<Self> {
        Ok(Self { stream: UnixStream::connect(socket(".socket2.sock")?)?, pending: Vec::new() })
    }

    /// The next whole line before `deadline`; None at the deadline, at the
    /// end of the stream or on an error.
    pub fn next_line(&mut self, deadline: Instant) -> Option<String> {
        loop {
            if let Some(end) = self.pending.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = self.pending.drain(..=end).collect();
                return Some(String::from_utf8_lossy(&line[..end]).into_owned());
            }
            let left = deadline.checked_duration_since(Instant::now()).filter(|left| !left.is_zero())?;
            self.stream.set_read_timeout(Some(left)).ok()?;
            let mut buf = [0u8; 4096];
            match self.stream.read(&mut buf) {
                Ok(0) | Err(_) => return None,
                Ok(n) => self.pending.extend_from_slice(&buf[..n]),
            }
        }
    }
}

fn socket(name: &str) -> io::Result<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR");
    let instance = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").filter(|instance| !instance.is_empty());
    match (runtime, instance) {
        (Some(runtime), Some(instance)) => Ok(PathBuf::from(runtime).join("hypr").join(instance).join(name)),
        _ => Err(io::Error::new(io::ErrorKind::NotFound, "not in a Hyprland session")),
    }
}

fn request(text: &str) -> io::Result<String> {
    let path = socket(".socket.sock")?;
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    stream.write_all(text.as_bytes())?;
    let mut reply = String::new();
    stream.take(REPLY_LIMIT).read_to_string(&mut reply)?;
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_list_should_have_no_errors() {
        assert_eq!(has_errors("[\n]\n"), Some(false));
    }

    #[test]
    fn a_list_of_one_empty_line_should_have_no_errors() {
        assert_eq!(has_errors("[\n\t\"\"\n]\n"), Some(false));
    }

    #[test]
    fn a_list_with_a_message_should_have_errors() {
        assert_eq!(has_errors("[\n\t\"custom.lua:3: unexpected symbol\"\n]\n"), Some(true));
    }

    #[test]
    fn a_reply_that_isnt_a_list_should_be_unreadable() {
        assert_eq!(has_errors("unknown request"), None);
    }
}
