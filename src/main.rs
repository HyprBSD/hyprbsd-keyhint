//! hyprbsd-keyhint: the key binding for a program opened from HyprBSD's
//! launcher, menu or bar, shown for a moment at the bottom of its window.
//!
//!   hyprbsd-keyhint launch [wofi option ...]   wofi's drun: start the entry picked, then its keys
//!   hyprbsd-keyhint show command [arg ...]     the keys of the binding that runs this command; prints
//!                                              "ready" once windows opening from then on are seen
//!   hyprbsd-keyhint hide                       close the card
//!
//! `hyprbsd-keyhint card` is the card's own process.
//!
//! No card while ~/.config/hyprbsd/keyhint-off exists.

mod card;
mod command;
mod hyprland;
mod launch;
mod observe;
mod place;
mod selection;
mod shortcuts;

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use command::Invocation;
use observe::Watch;

/// How long the card waits for the program's window before showing in the corner.
const GRACE: Duration = Duration::from_millis(700);
/// How long after the launch the card still moves to the program's window.
const WATCH: Duration = Duration::from_secs(5);

const USAGE: &str = "usage: hyprbsd-keyhint launch [wofi option ...]\n       \
                     hyprbsd-keyhint show command [arg ...]\n       hyprbsd-keyhint hide";

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let verb = args.next();
    let rest: Vec<OsString> = args.collect();
    match verb.as_deref().and_then(|verb| verb.to_str()) {
        Some("launch") => launch(&rest),
        Some("show") if !rest.is_empty() => show(&rest),
        Some("hide") if rest.is_empty() => to_exit_code(card::run(None)),
        Some("card") => {
            let args: Vec<String> = rest.iter().map(|arg| arg.to_string_lossy().into_owned()).collect();
            to_exit_code(card::run(card::Request::from_args(&args).as_ref()))
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(64)
        }
    }
}

fn launch(wofi_args: &[OsString]) -> ExitCode {
    let picked = match launch::pick(wofi_args) {
        Ok(Some(picked)) => picked,
        Ok(None) => return ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("hyprbsd-keyhint: wofi: {error}");
            return ExitCode::FAILURE;
        }
    };
    let entry = match launch::Entry::open(&picked) {
        Ok(entry) => entry,
        Err(error) => {
            eprintln!("hyprbsd-keyhint: {error}");
            return ExitCode::FAILURE;
        }
    };
    let hint = entry.started().and_then(|(started, classes)| Hint::prepare(&started, &classes));
    if let Err(error) = entry.start() {
        eprintln!("hyprbsd-keyhint: {}: {error}", picked.path);
        return ExitCode::FAILURE;
    }
    if let Some(hint) = hint {
        hint.show();
    }
    ExitCode::SUCCESS
}

fn show(command: &[OsString]) -> ExitCode {
    let line: Vec<String> = command.iter().map(|arg| arg.to_string_lossy().into_owned()).collect();
    let hint =
        Invocation::from_command(&shell_words(&line)).and_then(|started| Hint::prepare(&started, &started.classes()));
    let mut out = std::io::stdout();
    let _ = writeln!(out, "ready").and_then(|()| out.flush());
    if let Some(hint) = hint {
        hint.show();
    }
    ExitCode::SUCCESS
}

/// A card to show once the program's window is known.
struct Hint {
    asked: u128,
    caps: Vec<String>,
    watch: Option<(hyprland::Events, Watch)>,
    origin: Option<String>,
}

impl Hint {
    /// The keys for what's started, the time it was asked for, and
    /// Hyprland's events read from now on; None while the hints are off or
    /// no binding starts it.
    fn prepare(started: &Invocation, classes: &[String]) -> Option<Self> {
        let asked = card::now();
        if off() {
            return None;
        }
        let all = hyprland::shortcuts()?;
        let caps = shortcuts::best(&all, started).and_then(shortcuts::labels)?;
        let events = hyprland::Events::connect().ok();
        let baseline = events.as_ref().and_then(|_| hyprland::baseline());
        let (watch, origin) = match (events, baseline) {
            (Some(events), Some((before, origin))) => (Some((events, Watch::new(classes, before))), origin),
            _ => (None, None),
        };
        Some(Self { asked, caps, watch, origin })
    }

    /// Shows the card at the program's window when it opens within GRACE,
    /// else at the program's window already in focus or in the corner of
    /// the monitor the launch came from; a program's window opening later,
    /// up to WATCH after the launch, moves that card while it's still up.
    fn show(self) {
        let start = Instant::now();
        let id = format!("{}-{}", std::process::id(), self.asked);
        let mut request =
            card::Request { id, asked: self.asked, late: false, caps: self.caps, window: None, origin: self.origin };
        let Some((mut events, watch)) = self.watch else {
            spawn_card(&request);
            return;
        };
        let opened = next_window(&mut events, &watch, start + GRACE);
        if off() {
            return;
        }
        request.window = opened.clone().or_else(|| watch.already_focused());
        spawn_card(&request);
        if opened.is_none()
            && let Some(window) = next_window(&mut events, &watch, start + WATCH)
            && !off()
        {
            request.window = Some(window);
            request.late = true;
            spawn_card(&request);
        }
    }
}

/// The program's window, from the events before `deadline`.
fn next_window(events: &mut hyprland::Events, watch: &Watch, deadline: Instant) -> Option<String> {
    while let Some(line) = events.next_line(deadline) {
        if let Some(address) = observe::parse(&line).and_then(|event| watch.on(&event)) {
            return Some(address);
        }
    }
    None
}

/// Hands the request to the card's own process, which shows it or passes it
/// to the one showing the card, and reaps that process when it ends.
fn spawn_card(request: &card::Request) {
    let Ok(program) = std::env::current_exe() else { return };
    match Command::new(program).arg("card").args(request.to_args()).stdin(Stdio::null()).stdout(Stdio::null()).spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
        }
        Err(error) => eprintln!("hyprbsd-keyhint: the card: {error}"),
    }
}

fn off() -> bool {
    std::env::var_os("HOME").is_some_and(|home| PathBuf::from(home).join(".config/hyprbsd/keyhint-off").exists())
}

/// The arguments as one sh command line, each quoted.
fn shell_words(args: &[String]) -> String {
    args.iter().map(|arg| format!("'{}'", arg.replace('\'', "'\\''"))).collect::<Vec<_>>().join(" ")
}

fn to_exit_code(code: gtk4::glib::ExitCode) -> ExitCode {
    if code == gtk4::glib::ExitCode::SUCCESS { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
