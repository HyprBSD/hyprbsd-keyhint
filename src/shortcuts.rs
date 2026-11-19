//! The key bindings HyprBSD's Hyprland config lists through hyprbsd_shortcuts().

use crate::command::Invocation;

const HEADER: &str = "hyprbsd-shortcuts 1";

const SHIFT: u32 = 1;
const CTRL: u32 = 4;
const ALT: u32 = 8;
const SUPER: u32 = 64;

/// A key binding that runs a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    pub modmask: u32,
    pub key: String,
    pub command: String,
}

/// The bindings in hyprbsd_shortcuts()'s output, or None when it isn't that format.
pub fn parse(text: &str) -> Option<Vec<Shortcut>> {
    let mut lines = text.lines();
    if lines.next()? != HEADER {
        return None;
    }
    lines
        .map(|line| {
            let mut fields = line.splitn(3, '\t');
            let modmask = fields.next()?.parse().ok()?;
            let key = fields.next()?.to_owned();
            let command = unescape(fields.next()?)?;
            Some(Shortcut { modmask, key, command })
        })
        .collect()
}

fn unescape(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        out.push(match c {
            '\\' => match chars.next()? {
                '\\' => '\\',
                't' => '\t',
                'n' => '\n',
                _ => return None,
            },
            c => c,
        });
    }
    Some(out)
}

/// The key caps to show for a binding, modifiers first; None for media keys,
/// mouse buttons and modifiers other than Super, Ctrl, Alt and Shift.
pub fn labels(shortcut: &Shortcut) -> Option<Vec<String>> {
    let key = &shortcut.key;
    if shortcut.modmask & !(SUPER | CTRL | ALT | SHIFT) != 0 || key.starts_with("XF86") || key.starts_with("mouse") {
        return None;
    }
    let modifiers = [(SUPER, "SUPER"), (CTRL, "CTRL"), (ALT, "ALT"), (SHIFT, "SHIFT")];
    let mut caps: Vec<String> =
        modifiers.iter().filter(|(bit, _)| shortcut.modmask & bit != 0).map(|(_, name)| (*name).to_owned()).collect();
    caps.push(match key.as_str() {
        "Return" => "ENTER".to_owned(),
        _ => key.to_uppercase(),
    });
    Some(caps)
}

/// The binding to teach for what was started: one with caps to show and the
/// same invocation, fewest modifiers first.
pub fn best<'a>(shortcuts: &'a [Shortcut], started: &Invocation) -> Option<&'a Shortcut> {
    shortcuts
        .iter()
        .filter(|shortcut| labels(shortcut).is_some())
        .filter(|shortcut| Invocation::from_command(&shortcut.command).as_ref() == Some(started))
        .min_by(|a, b| (a.modmask.count_ones(), a.modmask, &a.key).cmp(&(b.modmask.count_ones(), b.modmask, &b.key)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shortcut(modmask: u32, key: &str, command: &str) -> Shortcut {
        Shortcut { modmask, key: key.to_owned(), command: command.to_owned() }
    }

    #[test]
    fn parse_should_read_each_line_after_the_header() {
        let text = "hyprbsd-shortcuts 1\n64\tReturn\tfoot\n72\tC\tgalculator";
        assert_eq!(parse(text), Some(vec![shortcut(64, "Return", "foot"), shortcut(72, "C", "galculator")]));
    }

    #[test]
    fn parse_should_accept_no_bindings() {
        assert_eq!(parse("hyprbsd-shortcuts 1"), Some(vec![]));
    }

    #[test]
    fn parse_should_unescape_backslash_tab_and_newline() {
        let parsed = parse("hyprbsd-shortcuts 1\n64\tT\tprintf 'a\\tb\\\\c\\nd'").unwrap();
        assert_eq!(parsed[0].command, "printf 'a\tb\\c\nd'");
    }

    #[test]
    fn parse_should_refuse_output_without_the_header() {
        assert_eq!(parse("false"), None);
    }

    #[test]
    fn parse_should_refuse_another_version() {
        assert_eq!(parse("hyprbsd-shortcuts 2\n64\tReturn\tfoot"), None);
    }

    #[test]
    fn parse_should_refuse_a_line_without_three_fields() {
        assert_eq!(parse("hyprbsd-shortcuts 1\n64\tReturn"), None);
    }

    #[test]
    fn parse_should_refuse_a_modmask_that_isnt_a_number() {
        assert_eq!(parse("hyprbsd-shortcuts 1\nSUPER\tReturn\tfoot"), None);
    }

    #[test]
    fn parse_should_refuse_an_unknown_escape() {
        assert_eq!(parse("hyprbsd-shortcuts 1\n64\tT\ta\\qb"), None);
    }

    #[test]
    fn labels_should_put_modifiers_in_a_fixed_order() {
        let caps = labels(&shortcut(SUPER | SHIFT | ALT | CTRL, "K", "x")).unwrap();
        assert_eq!(caps, ["SUPER", "CTRL", "ALT", "SHIFT", "K"]);
    }

    #[test]
    fn labels_should_call_return_enter() {
        assert_eq!(labels(&shortcut(SUPER, "Return", "foot")).unwrap(), ["SUPER", "ENTER"]);
    }

    #[test]
    fn labels_should_capitalise_letters_and_names() {
        assert_eq!(labels(&shortcut(SUPER, "space", "x")).unwrap(), ["SUPER", "SPACE"]);
    }

    #[test]
    fn labels_should_skip_media_keys() {
        assert_eq!(labels(&shortcut(0, "XF86Calculator", "galculator")), None);
    }

    #[test]
    fn labels_should_skip_mouse_buttons_and_wheel() {
        assert_eq!(labels(&shortcut(SUPER, "mouse_down", "x")), None);
    }

    #[test]
    fn labels_should_skip_caps_lock_and_other_modifiers() {
        assert_eq!(labels(&shortcut(SUPER | 2, "G", "gimp")), None);
    }

    #[test]
    fn best_should_find_the_binding_that_starts_the_same_program() {
        let all = [shortcut(SUPER, "Return", "foot"), shortcut(SUPER, "W", "firefox")];
        let started = Invocation::from_command("firefox").unwrap();
        assert_eq!(best(&all, &started), Some(&all[1]));
    }

    #[test]
    fn best_should_prefer_fewer_modifiers() {
        let all = [shortcut(SUPER | ALT, "C", "galculator"), shortcut(SUPER, "C", "galculator")];
        let started = Invocation::from_command("galculator").unwrap();
        assert_eq!(best(&all, &started), Some(&all[1]));
    }

    #[test]
    fn best_should_pass_over_keys_it_cant_show() {
        let all = [shortcut(0, "XF86Calculator", "galculator"), shortcut(SUPER | ALT, "C", "galculator")];
        let started = Invocation::from_command("galculator").unwrap();
        assert_eq!(best(&all, &started), Some(&all[1]));
    }

    #[test]
    fn best_should_break_ties_the_same_way_every_time() {
        let all = [shortcut(SUPER, "R", "wofi --show drun"), shortcut(SUPER, "SPACE", "wofi --show drun")];
        let started = Invocation::from_command("wofi --show drun").unwrap();
        assert_eq!(best(&all, &started), Some(&all[0]));
    }

    #[test]
    fn best_should_find_nothing_for_a_program_without_a_binding() {
        let all = [shortcut(SUPER, "Return", "foot")];
        let started = Invocation::from_command("gimp").unwrap();
        assert_eq!(best(&all, &started), None);
    }

    #[test]
    fn best_should_skip_bindings_whose_command_it_cant_read() {
        let all = [shortcut(SUPER, "G", "gimp | tee log")];
        let started = Invocation::from_command("gimp").unwrap();
        assert_eq!(best(&all, &started), None);
    }
}
