//! hyprbsd-keyhint: the key binding for a program opened from HyprBSD's
//! launcher, menu or bar, shown for a moment at the bottom of its window.
//!
//!   hyprbsd-keyhint launch [wofi option ...]   wofi's drun: start the entry picked, then its keys
//!   hyprbsd-keyhint show command [arg ...]     the keys of the binding that runs this command; prints
//!                                              "ready" once windows opening from then on are seen
//!   hyprbsd-keyhint hide                       close the card
//!   hyprbsd-keyhint entry address              the ID and name of the desktop entry a window is from;
//!                                              1 when it isn't known
//!   hyprbsd-keyhint bind id                    ask for keys to start desktop entry id and print them,
//!                                              as Hyprland's bind takes them; 1 on Escape or after a
//!                                              minute, 2 when there's no such entry, 3 with no display
//!   hyprbsd-keyhint start id                   start desktop entry id with no card, as a saved key does;
//!                                              2 when there's no such entry
//!
//! `hyprbsd-keyhint card` is the card's own process. `launch` and `start`
//! record the entry the first window they open is from, which `entry` reads.
//!
//! No card while ~/.config/hyprbsd/keyhint-off exists.

mod bind;
mod card;
mod command;
mod entries;
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
/// How long after the launch the card still moves to the program's window,
/// and a window of the program's opening is recorded.
const WATCH: Duration = Duration::from_secs(5);

const USAGE: &str = "usage: hyprbsd-keyhint launch [wofi option ...]\n       \
                     hyprbsd-keyhint show command [arg ...]\n       hyprbsd-keyhint hide\n       \
                     hyprbsd-keyhint entry address\n       hyprbsd-keyhint bind id\n       \
                     hyprbsd-keyhint start id";

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let verb = args.next();
    let rest: Vec<OsString> = args.collect();
    match verb.as_deref().and_then(|verb| verb.to_str()) {
        Some("launch") => launch(&rest),
        Some("show") if !rest.is_empty() => show(&rest),
        Some("hide") if rest.is_empty() => to_exit_code(card::run(None)),
        Some("entry") if rest.len() == 1 => entry(&rest[0].to_string_lossy()),
        Some("bind") if rest.len() == 1 => bind(&rest[0].to_string_lossy()),
        Some("start") if rest.len() == 1 => start(&rest[0].to_string_lossy()),
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
    start_entry(&entry, entry.started(), &picked.path)
}

fn start(id: &str) -> ExitCode {
    let Some(entry) = launch::Entry::by_id(id) else {
        eprintln!("hyprbsd-keyhint: {id}: no such desktop entry");
        return ExitCode::from(2);
    };
    start_entry(&entry, None, id)
}

/// Starts the entry, then shows the card for what it started, if any, and
/// records its first window.
fn start_entry(entry: &launch::Entry, started: Option<Invocation>, subject: &str) -> ExitCode {
    let hint = Hint::prepare(started.as_ref(), &entry.classes(), entry.id());
    if let Err(error) = entry.start() {
        eprintln!("hyprbsd-keyhint: {subject}: {error}");
        return ExitCode::FAILURE;
    }
    if let Some(hint) = hint {
        hint.show();
    }
    ExitCode::SUCCESS
}

fn show(command: &[OsString]) -> ExitCode {
    let line: Vec<String> = command.iter().map(|arg| arg.to_string_lossy().into_owned()).collect();
    let hint = Invocation::from_command(&shell_words(&line))
        .and_then(|started| Hint::prepare(Some(&started), &started.classes(), None));
    let mut out = std::io::stdout();
    let _ = writeln!(out, "ready").and_then(|()| out.flush());
    if let Some(hint) = hint {
        hint.show();
    }
    ExitCode::SUCCESS
}

/// Records that the window is from the entry, for `entry`, and forgets the
/// records of windows that are gone; whether it was recorded.
fn record(window: &str, entry: Option<&str>) -> bool {
    let Some(entry) = entry else { return false };
    let listed = std::time::SystemTime::now();
    let Some(clients) = hyprland::clients() else { return false };
    let Some(found) = entries::window(&clients, window) else { return false };
    entries::prune(&entries::addresses(&clients), listed);
    match entries::save(window, &entries::Record { id: entry.to_owned(), pid: found.pid }) {
        Ok(()) => true,
        Err(error) => {
            eprintln!("hyprbsd-keyhint: recording window {window}: {error}");
            false
        }
    }
}

fn entry(address: &str) -> ExitCode {
    let Some(window) = hyprland::clients().and_then(|clients| entries::window(&clients, address)) else {
        return ExitCode::FAILURE;
    };
    let candidates = launch::candidates();
    let recorded = entries::load(address).and_then(|record| entries::recorded(&record, &window, &candidates));
    let Some(found) = recorded.or_else(|| entries::best(&[&window.class, &window.initial_class], &candidates)) else {
        return ExitCode::FAILURE;
    };
    let name: String = found.name.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    println!("{}\t{name}", found.id);
    ExitCode::SUCCESS
}

fn bind(id: &str) -> ExitCode {
    let Some(name) = launch::name(id) else {
        eprintln!("hyprbsd-keyhint: {id}: no such desktop entry");
        return ExitCode::from(2);
    };
    bind::run(&name)
}

/// A card to show once the program's window is known, and the desktop entry
/// to record for that window.
struct Hint {
    asked: u128,
    caps: Option<Vec<String>>,
    watch: Option<(hyprland::Events, Watch)>,
    origin: Option<String>,
    entry: Option<String>,
}

impl Hint {
    /// The keys for what's started (none while the hints are off or no
    /// binding starts it), the time it was asked for, and Hyprland's events
    /// read from now on; None with no card to show and no window to record.
    fn prepare(started: Option<&Invocation>, classes: &[String], entry: Option<String>) -> Option<Self> {
        let asked = card::now();
        let caps = started.filter(|_| !off()).and_then(|started| {
            let all = hyprland::shortcuts()?;
            shortcuts::best(&all, started, launch::starts).and_then(shortcuts::labels)
        });
        if caps.is_none() && entry.is_none() {
            return None;
        }
        let events = hyprland::Events::connect().ok();
        let baseline = events.as_ref().and_then(|_| hyprland::baseline());
        let (watch, origin) = match (events, baseline) {
            (Some(events), Some((before, origin))) => (Some((events, Watch::new(classes, before))), origin),
            _ => (None, None),
        };
        if caps.is_none() && watch.is_none() {
            return None;
        }
        Some(Self { asked, caps, watch, origin, entry })
    }

    /// Shows the card at the program's window when it opens within GRACE,
    /// else at the program's window already in focus or in the corner of
    /// the monitor the launch came from; a program's window opening later,
    /// up to WATCH after the launch, moves that card while it's still up.
    /// Records the first new window of the program's up to WATCH, whatever the
    /// card does.
    fn show(self) {
        let start = Instant::now();
        let entry = self.entry.as_deref();
        let Some(caps) = self.caps else {
            if let Some((mut events, watch)) = self.watch {
                record_next_new(&mut events, &watch, start + WATCH, entry);
            }
            return;
        };
        let id = format!("{}-{}", std::process::id(), self.asked);
        let mut request = card::Request { id, asked: self.asked, late: false, caps, window: None, origin: self.origin };
        let Some((mut events, watch)) = self.watch else {
            spawn_card(&request);
            return;
        };
        let opened = next_window(&mut events, &watch, start + GRACE);
        let mut recorded = opened.as_ref().is_some_and(|window| watch.is_new(window) && record(window, entry));
        if !off() {
            request.window = opened.clone().or_else(|| watch.already_focused());
            spawn_card(&request);
            if opened.is_none()
                && let Some(window) = next_window(&mut events, &watch, start + WATCH)
            {
                recorded = watch.is_new(&window) && record(&window, entry);
                if !off() {
                    request.window = Some(window);
                    request.late = true;
                    spawn_card(&request);
                }
            }
        }
        if !recorded {
            record_next_new(&mut events, &watch, start + WATCH, entry);
        }
    }
}

/// Records the first new window of the program's from the events before
/// `deadline`; nothing to wait for without an entry.
fn record_next_new(events: &mut hyprland::Events, watch: &Watch, deadline: Instant, entry: Option<&str>) {
    if entry.is_none() {
        return;
    }
    while let Some(window) = next_window(events, watch, deadline) {
        if watch.is_new(&window) && record(&window, entry) {
            return;
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
