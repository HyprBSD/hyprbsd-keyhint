//! Which desktop entry a window is from: the one its launch recorded, or the
//! one its class names.

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::command::TERMINAL_CLASS;

/// A desktop entry shown in menus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The entry's ID without ".desktop".
    pub id: String,
    pub name: String,
    pub wm_class: Option<String>,
    /// The program its Exec runs; None when a terminal runs it.
    pub program: Option<String>,
    /// Every class its windows may have, the terminal's included.
    pub classes: Vec<String>,
}

/// The entry a window of these classes (its class and initial class) is
/// from: the one whose StartupWMClass or ID is a class, else whose ID's last
/// dotted part is, else whose program is. None when two match equally well,
/// or the classes are empty or the terminal's.
pub fn best<'a>(classes: &[&str], candidates: &'a [Candidate]) -> Option<&'a Candidate> {
    let classes: Vec<String> = classes
        .iter()
        .filter(|class| !class.is_empty() && !class.eq_ignore_ascii_case(TERMINAL_CLASS))
        .map(|class| class.to_lowercase())
        .collect();
    if classes.is_empty() {
        return None;
    }
    let is_class = |name: &str| classes.iter().any(|class| class == &name.to_lowercase());
    let score = |candidate: &Candidate| {
        let last = candidate.id.rsplit_once('.').map_or(candidate.id.as_str(), |(_, last)| last);
        if candidate.wm_class.as_deref().is_some_and(is_class) || is_class(&candidate.id) {
            3
        } else if is_class(last) {
            2
        } else if candidate.program.as_deref().is_some_and(is_class) {
            1
        } else {
            0
        }
    };
    let top = candidates.iter().map(score).max().filter(|&top| top > 0)?;
    let mut matching = candidates.iter().filter(|candidate| score(candidate) == top);
    let found = matching.next()?;
    matching.next().is_none().then_some(found)
}

/// The entry a record names, when the record is for this window's process
/// and the entry's windows may have the window's class or initial class.
pub fn recorded<'a>(record: &Record, window: &Window, candidates: &'a [Candidate]) -> Option<&'a Candidate> {
    if record.pid != window.pid {
        return None;
    }
    let is_window_s = |class: &String| {
        [&window.class, &window.initial_class].iter().any(|own| !own.is_empty() && class.eq_ignore_ascii_case(own))
    };
    candidates
        .iter()
        .find(|candidate| candidate.id == record.id)
        .filter(|candidate| candidate.classes.iter().any(is_window_s))
}

/// A window as Hyprland's `j/clients` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub class: String,
    pub initial_class: String,
    pub pid: i64,
}

/// The window at `address` in a `j/clients` reply.
pub fn window(clients: &str, address: &str) -> Option<Window> {
    let clients: Vec<serde_json::Value> = serde_json::from_str(clients).ok()?;
    let client = clients.iter().find(|client| client["address"].as_str() == Some(address))?;
    let text = |field: &str| client[field].as_str().unwrap_or_default().to_owned();
    Some(Window {
        class: text("class"),
        initial_class: text("initialClass"),
        pid: client["pid"].as_i64().unwrap_or(-1),
    })
}

/// The addresses in a `j/clients` reply.
pub fn addresses(clients: &str) -> Vec<String> {
    let clients: Vec<serde_json::Value> = serde_json::from_str(clients).unwrap_or_default();
    clients.iter().filter_map(|client| client["address"].as_str().map(str::to_owned)).collect()
}

/// The entry a launch started a window from, and the window's process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub id: String,
    pub pid: i64,
}

impl Record {
    /// From a record file: the ID, then the process ID, a line each.
    pub fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        let id = lines.next().filter(|id| !id.is_empty())?.to_owned();
        let pid = lines.next()?.parse().ok()?;
        Some(Self { id, pid })
    }

    pub fn to_text(&self) -> String {
        format!("{}\n{}\n", self.id, self.pid)
    }
}

/// The directory of records, in the session's runtime directory.
fn directory() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|runtime| PathBuf::from(runtime).join("hyprbsd-keyhint/windows"))
}

/// Whether a name is a window address: 0x and hexadecimal digits.
fn is_address(name: &str) -> bool {
    name.strip_prefix("0x").is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The record file for a window; None for a name that isn't an address, or
/// without XDG_RUNTIME_DIR.
fn path(address: &str) -> Option<PathBuf> {
    is_address(address).then(directory).flatten().map(|directory| directory.join(address))
}

/// Records the entry a window is from.
pub fn save(address: &str, record: &Record) -> io::Result<()> {
    let path = path(address)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a window address, or no XDG_RUNTIME_DIR"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let new = path.with_extension("new");
    std::fs::write(&new, record.to_text())?;
    std::fs::rename(&new, &path)
}

/// The record for a window, if its launch made one.
pub fn load(address: &str) -> Option<Record> {
    Record::parse(&std::fs::read_to_string(path(address)?).ok()?)
}

/// Removes the records of windows not in `live`, the window list read at
/// `listed`.
pub fn prune(live: &[String], listed: SystemTime) {
    if let Some(directory) = directory() {
        prune_in(&directory, live, listed);
    }
}

/// Removes the records in `directory` of windows not in `live`, leaving
/// records saved since `listed` and other files, such as a record another
/// launch is still writing.
fn prune_in(directory: &Path, live: &[String], listed: SystemTime) {
    let Ok(entries) = std::fs::read_dir(directory) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let saved_since = entry.metadata().and_then(|metadata| metadata.modified()).is_ok_and(|saved| saved > listed);
        if is_address(name) && !live.iter().any(|address| address == name) && !saved_since {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An entry whose windows may have its program's class (the terminal's
    /// when there's no program), its StartupWMClass and its ID.
    fn candidate(id: &str, wm_class: Option<&str>, program: Option<&str>) -> Candidate {
        let classes = [program.unwrap_or(TERMINAL_CLASS)].into_iter().chain(wm_class).chain([id]);
        Candidate {
            id: id.to_owned(),
            name: id.to_owned(),
            wm_class: wm_class.map(str::to_owned),
            program: program.map(str::to_owned),
            classes: classes.map(str::to_owned).collect(),
        }
    }

    fn a_window(class: &str, initial_class: &str, pid: i64) -> Window {
        Window { class: class.into(), initial_class: initial_class.into(), pid }
    }

    fn a_record(id: &str, pid: i64) -> Record {
        Record { id: id.into(), pid }
    }

    #[test]
    fn a_record_should_name_its_entry_for_its_window() {
        let all = [candidate("btop", None, None)];
        let found = recorded(&a_record("btop", 7), &a_window("foot", "foot", 7), &all);
        assert_eq!(found.map(|candidate| candidate.id.as_str()), Some("btop"));
    }

    #[test]
    fn a_record_should_hold_when_the_window_s_class_has_changed_since() {
        let all = [candidate("steam", None, Some("steam"))];
        let found = recorded(&a_record("steam", 7), &a_window("steam_app_1", "steam", 7), &all);
        assert_eq!(found.map(|candidate| candidate.id.as_str()), Some("steam"));
    }

    #[test]
    fn a_record_for_another_process_should_name_nothing() {
        let all = [candidate("btop", None, None)];
        assert_eq!(recorded(&a_record("btop", 7), &a_window("foot", "foot", 8), &all), None);
    }

    #[test]
    fn a_record_whose_entry_has_another_class_should_name_nothing() {
        let all = [candidate("btop", None, None)];
        assert_eq!(recorded(&a_record("btop", 7), &a_window("mpv", "mpv", 7), &all), None);
    }

    #[test]
    fn a_record_for_an_entry_not_shown_should_name_nothing() {
        let all = [candidate("thunar", None, Some("thunar"))];
        assert_eq!(recorded(&a_record("btop", 7), &a_window("foot", "foot", 7), &all), None);
    }

    fn found<'a>(classes: &[&str], candidates: &'a [Candidate]) -> Option<&'a str> {
        best(classes, candidates).map(|candidate| candidate.id.as_str())
    }

    #[test]
    fn an_entry_named_by_the_class_should_be_found() {
        let all = [candidate("thunar", None, Some("thunar")), candidate("thunar-bulk-rename", None, Some("thunar"))];
        assert_eq!(found(&["thunar", "thunar"], &all), Some("thunar"));
    }

    #[test]
    fn startup_wm_class_should_name_the_entry() {
        let all = [candidate("org.example.Tool", Some("tool-window"), Some("tool"))];
        assert_eq!(found(&["tool-window", "tool-window"], &all), Some("org.example.Tool"));
    }

    #[test]
    fn the_class_should_match_without_case() {
        let all = [candidate("vlc", None, Some("vlc"))];
        assert_eq!(found(&["VLC", "VLC"], &all), Some("vlc"));
    }

    #[test]
    fn a_reverse_domain_id_s_last_part_should_match() {
        let all = [candidate("org.gnome.Evince", None, Some("evince"))];
        assert_eq!(found(&["evince", "evince"], &all), Some("org.gnome.Evince"));
    }

    #[test]
    fn the_program_should_match_when_nothing_else_does() {
        let all = [candidate("designer", None, Some("designer6"))];
        assert_eq!(found(&["designer6", "designer6"], &all), Some("designer"));
    }

    #[test]
    fn the_initial_class_should_match_too() {
        let all = [candidate("steam", None, Some("steam"))];
        assert_eq!(found(&["steam_app_123", "steam"], &all), Some("steam"));
    }

    #[test]
    fn two_entries_matching_equally_should_name_neither() {
        let all = [candidate("one", Some("shared"), None), candidate("two", Some("shared"), None)];
        assert_eq!(found(&["shared", "shared"], &all), None);
    }

    #[test]
    fn an_empty_class_should_name_no_entry() {
        let all = [candidate("hyprbsd-software", None, Some("hyprbsd-software"))];
        assert_eq!(found(&["", ""], &all), None);
    }

    #[test]
    fn the_terminal_s_class_should_name_no_entry() {
        let all = [candidate("foot", None, Some("foot"))];
        assert_eq!(found(&["foot", "foot"], &all), None);
    }

    #[test]
    fn a_class_no_entry_has_should_name_none() {
        let all = [candidate("thunar", None, Some("thunar"))];
        assert_eq!(found(&["mpv", "mpv"], &all), None);
    }

    #[test]
    fn a_window_should_be_read_from_the_client_list() {
        let clients = r#"[{"address": "0xa", "class": "a"}, {"address": "0xb1", "class": "gimp", "initialClass": "Gimp", "pid": 42}]"#;
        assert_eq!(
            window(clients, "0xb1"),
            Some(Window { class: "gimp".into(), initial_class: "Gimp".into(), pid: 42 })
        );
    }

    #[test]
    fn a_window_not_listed_should_be_none() {
        assert_eq!(window(r#"[{"address": "0xa"}]"#, "0xb"), None);
    }

    #[test]
    fn a_record_should_read_back() {
        let record = Record { id: "org.gimp.GIMP".into(), pid: 42 };
        assert_eq!(Record::parse(&record.to_text()), Some(record));
    }

    #[test]
    fn a_record_without_a_process_should_not_be_read() {
        assert_eq!(Record::parse("gimp\n"), None);
    }

    #[test]
    fn an_address_that_isnt_hexadecimal_should_have_no_record_file() {
        assert_eq!(path("0x../../x"), None);
    }

    #[test]
    fn an_address_without_0x_should_have_no_record_file() {
        assert_eq!(path("b1"), None);
    }

    /// The files left in a new directory holding `before`, then `after`
    /// written once the window list was read, after pruning with `live`.
    fn left_after_pruning(test: &str, before: &[&str], after: &[&str], live: &[&str]) -> Vec<String> {
        let directory = std::env::temp_dir().join(format!("keyhint-{test}-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        for name in before {
            std::fs::write(directory.join(name), "x\n1\n").unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        let listed = std::time::SystemTime::now();
        std::thread::sleep(std::time::Duration::from_millis(20));
        for name in after {
            std::fs::write(directory.join(name), "x\n1\n").unwrap();
        }
        let live: Vec<String> = live.iter().map(|address| (*address).to_owned()).collect();
        prune_in(&directory, &live, listed);
        let mut left: Vec<String> = std::fs::read_dir(&directory)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        std::fs::remove_dir_all(&directory).unwrap();
        left
    }

    #[test]
    fn pruning_should_keep_live_records_and_files_still_being_written() {
        assert_eq!(left_after_pruning("prune", &["0x1", "0x2", "0x3.new"], &[], &["0x1"]), ["0x1", "0x3.new"]);
    }

    #[test]
    fn pruning_should_keep_a_record_saved_after_the_window_list_was_read() {
        assert_eq!(left_after_pruning("prune-late", &["0x2"], &["0x4"], &[]), ["0x4"]);
    }
}
