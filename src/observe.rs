//! Which window is the program's: one the launch opens, or one of its own
//! that the launch brings forward, told by the program's window classes.

use std::collections::HashMap;

/// An event from Hyprland's event socket that can name the program's window.
#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    Opened { address: String, class: String },
    Focused { address: String },
}

/// The event in one line of the event socket, if it's one of those.
pub fn parse(line: &str) -> Option<Event> {
    let (name, data) = line.split_once(">>")?;
    match name {
        "openwindow" => {
            let mut fields = data.splitn(4, ',');
            let address = fields.next().filter(|address| !address.is_empty())?;
            let class = fields.nth(1)?;
            Some(Event::Opened { address: format!("0x{address}"), class: class.to_owned() })
        }
        "activewindowv2" if !data.is_empty() => Some(Event::Focused { address: format!("0x{data}") }),
        _ => None,
    }
}

/// The windows before the launch, by address with their classes, and the
/// one in focus.
#[derive(Debug, Default)]
pub struct Baseline {
    pub classes: HashMap<String, String>,
    pub focused: Option<String>,
}

/// The baseline from `j/clients` and `j/activewindow` replies; None when
/// either isn't Hyprland's JSON.
pub fn baseline(clients: &str, active: &str) -> Option<Baseline> {
    let clients: Vec<serde_json::Value> = serde_json::from_str(clients).ok()?;
    let active: serde_json::Value = serde_json::from_str(active).ok()?;
    let classes = clients
        .iter()
        .filter_map(|client| Some((client["address"].as_str()?.to_owned(), client["class"].as_str()?.to_owned())))
        .collect();
    Some(Baseline { classes, focused: active["address"].as_str().map(str::to_owned) })
}

/// Looks for the program's window among events after the launch.
pub struct Watch {
    classes: Vec<String>,
    baseline: Baseline,
}

impl Watch {
    pub fn new(classes: &[String], baseline: Baseline) -> Self {
        Self { classes: classes.iter().map(|class| class.to_lowercase()).collect(), baseline }
    }

    /// The program's window, when the event shows it.
    pub fn on(&self, event: &Event) -> Option<String> {
        let before = &self.baseline;
        match event {
            Event::Opened { address, class } if self.is_new(address) && self.is_program(class) => Some(address.clone()),
            Event::Focused { address }
                if before.focused.as_ref() != Some(address)
                    && before.classes.get(address).is_some_and(|class| self.is_program(class)) =>
            {
                Some(address.clone())
            }
            _ => None,
        }
    }

    /// The window in focus before the launch, when it's the program's.
    pub fn already_focused(&self) -> Option<String> {
        let focused = self.baseline.focused.as_ref()?;
        self.baseline.classes.get(focused).is_some_and(|class| self.is_program(class)).then(|| focused.clone())
    }

    /// Whether the window wasn't there before the launch.
    pub fn is_new(&self, address: &str) -> bool {
        !self.baseline.classes.contains_key(address)
    }

    fn is_program(&self, class: &str) -> bool {
        self.classes.contains(&class.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn before(windows: &[(&str, &str)], focused: Option<&str>) -> Baseline {
        Baseline {
            classes: windows.iter().map(|(a, c)| ((*a).to_owned(), (*c).to_owned())).collect(),
            focused: focused.map(str::to_owned),
        }
    }

    fn watch(classes: &[&str], baseline: Baseline) -> Watch {
        Watch::new(&classes.iter().map(|c| (*c).to_owned()).collect::<Vec<_>>(), baseline)
    }

    fn opened(address: &str, class: &str) -> Event {
        Event::Opened { address: address.into(), class: class.into() }
    }

    fn focused(address: &str) -> Event {
        Event::Focused { address: address.into() }
    }

    #[test]
    fn a_window_listed_before_the_launch_should_not_be_new() {
        assert!(!watch(&["thunar"], before(&[("0x6", "thunar")], None)).is_new("0x6"));
    }

    #[test]
    fn a_window_not_listed_before_the_launch_should_be_new() {
        assert!(watch(&["thunar"], before(&[("0x6", "thunar")], None)).is_new("0x7"));
    }

    #[test]
    fn parse_should_read_an_opened_window() {
        assert_eq!(parse("openwindow>>2acb11211c00,1,foot,foot"), Some(opened("0x2acb11211c00", "foot")));
    }

    #[test]
    fn parse_should_keep_commas_in_the_title() {
        assert_eq!(parse("openwindow>>2a,2,firefox,a, b, c"), Some(opened("0x2a", "firefox")));
    }

    #[test]
    fn parse_should_read_a_window_brought_into_focus() {
        assert_eq!(parse("activewindowv2>>2acb11211c00"), Some(focused("0x2acb11211c00")));
    }

    #[test]
    fn parse_should_skip_focus_on_no_window() {
        assert_eq!(parse("activewindowv2>>"), None);
    }

    #[test]
    fn parse_should_skip_an_opened_window_without_a_class_field() {
        assert_eq!(parse("openwindow>>2a,1"), None);
    }

    #[test]
    fn parse_should_skip_other_events() {
        assert_eq!(parse("workspace>>2"), None);
    }

    #[test]
    fn baseline_should_keep_each_window_s_class_and_the_one_in_focus() {
        let found = baseline(
            r#"[{"address": "0x3", "class": "foot"}, {"address": "0x6", "class": "thunar"}]"#,
            r#"{"address": "0x6", "class": "thunar"}"#,
        )
        .unwrap();
        assert_eq!(
            (found.classes.get("0x3").map(String::as_str), found.focused.as_deref()),
            (Some("foot"), Some("0x6"))
        );
    }

    #[test]
    fn baseline_should_have_no_focus_when_no_window_has_it() {
        assert_eq!(baseline("[]", "{}").unwrap().focused, None);
    }

    #[test]
    fn baseline_should_refuse_replies_that_arent_json() {
        assert!(baseline("unknown request", "{}").is_none());
    }

    #[test]
    fn a_new_window_of_the_program_s_class_should_be_its_window() {
        let w = watch(&["thunar"], before(&[], None));
        assert_eq!(w.on(&opened("0x5", "thunar")).as_deref(), Some("0x5"));
    }

    #[test]
    fn classes_should_match_whatever_their_case() {
        let w = watch(&["firefox"], before(&[], None));
        assert_eq!(w.on(&opened("0x5", "Firefox")).as_deref(), Some("0x5"));
    }

    #[test]
    fn another_program_s_window_opening_first_should_be_passed_over() {
        let w = watch(&["thunar"], before(&[], None));
        assert_eq!(w.on(&opened("0x4", "steam")), None);
    }

    #[test]
    fn a_window_that_was_already_there_should_not_count_as_opened() {
        let w = watch(&["thunar"], before(&[("0x5", "thunar")], None));
        assert_eq!(w.on(&opened("0x5", "thunar")), None);
    }

    #[test]
    fn focus_returning_to_the_window_that_had_it_should_be_passed_over() {
        let w = watch(&["thunar"], before(&[("0x5", "thunar")], Some("0x5")));
        assert_eq!(w.on(&focused("0x5")), None);
    }

    #[test]
    fn the_program_s_own_window_brought_forward_should_be_its_window() {
        let w = watch(&["hyprbsd-network"], before(&[("0x3", "foot"), ("0x6", "hyprbsd-network")], Some("0x3")));
        assert_eq!(w.on(&focused("0x6")).as_deref(), Some("0x6"));
    }

    #[test]
    fn focus_on_another_program_s_window_should_be_passed_over() {
        let w = watch(&["hyprbsd-network"], before(&[("0x3", "foot"), ("0x7", "steam")], Some("0x3")));
        assert_eq!(w.on(&focused("0x7")), None);
    }

    #[test]
    fn the_program_s_window_already_in_focus_should_be_its_window() {
        let w = watch(&["hyprbsd-software"], before(&[("0x8", "hyprbsd-software")], Some("0x8")));
        assert_eq!(w.already_focused().as_deref(), Some("0x8"));
    }

    #[test]
    fn another_program_in_focus_should_not_be_the_program_s_window() {
        let w = watch(&["hyprbsd-software"], before(&[("0x3", "foot")], Some("0x3")));
        assert_eq!(w.already_focused(), None);
    }
}
