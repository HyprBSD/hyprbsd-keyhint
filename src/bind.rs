//! The dialog that asks for keys to start a program: Super and a key, not
//! keys another binding has. Prints them as Hyprland's bind takes them.

use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};
use gtk4_layer_shell::{KeyboardMode, Layer, LayerShell};

use crate::card;
use crate::hyprland;
use crate::shortcuts::{self, ALT, CTRL, SHIFT, SUPER};

const APP_ID: &str = "org.hyprbsd.keyhint.bind";
/// How long the dialog waits for keys.
const WAIT: Duration = Duration::from_secs(60);

const STYLE: &str = "
window.keyhint-bind { background: none; }
.dialog {
    background: #14161b;
    border: 2px solid #ef6f3c;
    border-radius: 6px;
    padding: 20px 28px;
    font-family: \"Noto Sans\";
}
.title { color: #e4e6eb; font-size: 15pt; font-weight: bold; }
.lead { color: #8b919c; font-size: 11pt; }
.status { color: #ef6f3c; font-size: 11pt; }
";

/// Asks for the keys to start `name`; prints them and exits 0, or exits 1
/// on Escape or after WAIT, 3 when there's no display to show it on. No keys
/// are accepted while Hyprland's bindings can't be read.
pub fn run(name: &str) -> ExitCode {
    if let Err(error) = gtk4::init() {
        eprintln!("hyprbsd-keyhint: the keys dialog: {error}");
        return ExitCode::from(3);
    }
    let app = gtk4::Application::builder().application_id(APP_ID).flags(gio::ApplicationFlags::NON_UNIQUE).build();
    let chosen: Rc<RefCell<Option<String>>> = Rc::default();
    let name = name.to_owned();
    let result = chosen.clone();
    app.connect_activate(move |app| show(app, &name, result.clone()));
    app.run_with_args(&["hyprbsd-keyhint"]);
    match chosen.take() {
        Some(keys) => {
            println!("{keys}");
            ExitCode::SUCCESS
        }
        None => ExitCode::FAILURE,
    }
}

fn show(app: &gtk4::Application, name: &str, chosen: Rc<RefCell<Option<String>>>) {
    card::add_style(STYLE);
    let window = gtk4::ApplicationWindow::new(app);
    window.add_css_class("keyhint-bind");
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_namespace(Some("hyprbsd-keyhint-bind"));
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    // Asks the compositor for every key while the dialog has the keyboard,
    // bound keys included.
    window.connect_map(|window| {
        if let Some(toplevel) = window.surface().and_downcast::<gdk::Toplevel>() {
            toplevel.inhibit_system_shortcuts(None::<&gdk::Event>);
        }
    });

    let dialog = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    dialog.add_css_class("dialog");
    let title = gtk4::Label::new(Some(&format!("Keys for {name}")));
    title.add_css_class("title");
    let lead = gtk4::Label::new(Some("Press Super and a key. Escape cancels."));
    lead.add_css_class("lead");
    let status = gtk4::Label::new(None);
    status.add_css_class("status");
    dialog.append(&title);
    dialog.append(&lead);
    dialog.append(&status);
    window.set_child(Some(&dialog));

    let keys = gtk4::EventControllerKey::new();
    let quit = app.clone();
    keys.connect_key_pressed(move |controller, key, keycode, state| {
        let modmask = modmask(state);
        if key == gdk::Key::Escape && modmask == 0 {
            quit.quit();
            return glib::Propagation::Stop;
        }
        let Some(name) = key.name() else { return glib::Propagation::Proceed };
        if is_modifier(&name) {
            return glib::Propagation::Proceed;
        }
        if modmask & SUPER == 0 {
            status.set_text("Hold Super as well.");
            return glib::Propagation::Stop;
        }
        // The key without Shift or the layout's other levels: 1, not !.
        let base = controller
            .widget()
            .and_then(|widget| {
                widget.display().translate_key(keycode, gdk::ModifierType::empty(), controller.group() as i32)
            })
            .and_then(|(base, ..)| base.name())
            .unwrap_or(name);
        let key = key_name(&base);
        match hyprland::binds().and_then(|binds| check(&binds, modmask, &key, keycode)) {
            Some(Keys::Free) => {
                chosen.replace(Some(keys_for(modmask, &key)));
                quit.quit();
            }
            Some(Keys::Taken(other)) => {
                status.set_text(&format!("{} is taken: {other}. Press other keys.", readable(modmask, &key)))
            }
            None => status.set_text("Hyprland's bindings can't be read to check the keys."),
        }
        glib::Propagation::Stop
    });
    window.add_controller(keys);
    let quit = app.clone();
    glib::timeout_add_local_once(WAIT, move || quit.quit());
    window.present();
}

/// Hyprland's modmask for the modifiers held.
fn modmask(state: gdk::ModifierType) -> u32 {
    [
        (gdk::ModifierType::SUPER_MASK, SUPER),
        (gdk::ModifierType::CONTROL_MASK, CTRL),
        (gdk::ModifierType::ALT_MASK, ALT),
        (gdk::ModifierType::SHIFT_MASK, SHIFT),
    ]
    .iter()
    .filter(|(mask, _)| state.contains(*mask))
    .fold(0, |modmask, (_, bit)| modmask | bit)
}

/// Whether an xkb key name is a modifier's own key.
fn is_modifier(name: &str) -> bool {
    ["Shift_", "Control_", "Alt_", "Meta_", "Super_", "Hyper_", "ISO_Level"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
        || ["Caps_Lock", "Num_Lock", "Scroll_Lock"].contains(&name)
}

/// The key's name as bindings give it: a letter in capitals, any other as
/// xkb names it.
fn key_name(name: &str) -> String {
    match name.as_bytes() {
        [letter] if letter.is_ascii_alphabetic() => name.to_ascii_uppercase(),
        _ => name.to_owned(),
    }
}

/// The keys as Hyprland's bind takes them: "SUPER + ALT + G".
fn keys_for(modmask: u32, key: &str) -> String {
    let mut words = shortcuts::modifiers(modmask);
    words.push(key.to_owned());
    words.join(" + ")
}

/// The keys as people read them: "Super + Alt + G", Return as Enter.
fn readable(modmask: u32, key: &str) -> String {
    let mut words: Vec<String> =
        shortcuts::modifiers(modmask).iter().map(|name| name[..1].to_owned() + &name[1..].to_lowercase()).collect();
    words.push(if key == "Return" { "Enter".to_owned() } else { key.to_owned() });
    words.join(" + ")
}

/// Whether keys are free, or what has them.
#[derive(Debug, PartialEq, Eq)]
enum Keys {
    Free,
    Taken(String),
}

/// Whether a binding outside any submap has these keys, from a `j/binds`
/// reply, named by its description or its dispatcher and argument; None when
/// the reply isn't a list of bindings.
fn check(binds: &str, modmask: u32, key: &str, keycode: u32) -> Option<Keys> {
    let binds: Vec<serde_json::Value> = serde_json::from_str(binds).ok()?;
    let Some(bind) = binds.iter().find(|bind| {
        let bound = bind["key"].as_str().unwrap_or_default();
        bind["submap"].as_str().unwrap_or_default().is_empty()
            && bind["modmask"].as_u64() == Some(u64::from(modmask))
            && (bound.eq_ignore_ascii_case(key)
                || (bound.is_empty() && bind["keycode"].as_u64() == Some(u64::from(keycode))))
    }) else {
        return Some(Keys::Free);
    };
    let description = bind["description"].as_str().unwrap_or_default().trim();
    if !description.is_empty() {
        return Some(Keys::Taken(description.to_owned()));
    }
    let what =
        format!("{} {}", bind["dispatcher"].as_str().unwrap_or_default(), bind["arg"].as_str().unwrap_or_default());
    Some(Keys::Taken(if what.trim().is_empty() { "another binding".to_owned() } else { what.trim().to_owned() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_letter_should_be_a_capital() {
        assert_eq!(key_name("g"), "G");
    }

    #[test]
    fn another_key_should_keep_xkb_s_name() {
        assert_eq!(key_name("comma"), "comma");
    }

    #[test]
    fn keys_should_put_modifiers_first_in_hyprland_s_form() {
        assert_eq!(keys_for(SUPER | SHIFT | ALT, "G"), "SUPER + ALT + SHIFT + G");
    }

    #[test]
    fn readable_keys_should_be_capitalised_words() {
        assert_eq!(readable(SUPER | CTRL, "F5"), "Super + Ctrl + F5");
    }

    #[test]
    fn shift_s_own_key_should_be_a_modifier() {
        assert!(is_modifier("Shift_L"));
    }

    #[test]
    fn super_s_own_key_should_be_a_modifier() {
        assert!(is_modifier("Super_R"));
    }

    #[test]
    fn a_letter_should_not_be_a_modifier() {
        assert!(!is_modifier("g"));
    }

    const BINDS: &str = r#"[
        {"modmask": 65, "submap": "", "key": "W", "keycode": 0, "mouse": false,
         "description": "Window menu", "dispatcher": "exec", "arg": "window-menu"},
        {"modmask": 64, "submap": "", "key": "Q", "keycode": 0, "mouse": false,
         "description": "", "dispatcher": "killactive", "arg": ""},
        {"modmask": 64, "submap": "resize", "key": "H", "keycode": 0, "mouse": false, "description": "Resize"},
        {"modmask": 64, "submap": "", "key": "", "keycode": 10, "mouse": false, "description": "Workspace 1"},
        {"modmask": 64, "submap": "", "key": "mouse:272", "keycode": 0, "mouse": true, "description": "Move"}
    ]"#;

    #[test]
    fn keys_bound_should_be_taken_by_their_description() {
        assert_eq!(check(BINDS, SUPER | SHIFT, "W", 25), Some(Keys::Taken("Window menu".into())));
    }

    #[test]
    fn a_key_should_match_without_case() {
        assert_eq!(check(BINDS, SUPER | SHIFT, "w", 25), Some(Keys::Taken("Window menu".into())));
    }

    #[test]
    fn a_binding_without_a_description_should_be_named_by_its_dispatcher() {
        assert_eq!(check(BINDS, SUPER, "Q", 24), Some(Keys::Taken("killactive".into())));
    }

    #[test]
    fn a_binding_by_key_code_should_take_that_key() {
        assert_eq!(check(BINDS, SUPER, "1", 10), Some(Keys::Taken("Workspace 1".into())));
    }

    #[test]
    fn other_modifiers_should_leave_the_keys_free() {
        assert_eq!(check(BINDS, SUPER | ALT, "W", 25), Some(Keys::Free));
    }

    #[test]
    fn a_submap_s_binding_should_leave_the_keys_free() {
        assert_eq!(check(BINDS, SUPER, "H", 43), Some(Keys::Free));
    }

    #[test]
    fn a_reply_that_isnt_a_list_of_bindings_should_tell_nothing() {
        assert_eq!(check("unknown request", SUPER, "G", 42), None);
    }

    #[test]
    fn return_should_read_as_enter() {
        assert_eq!(readable(SUPER, "Return"), "Super + Enter");
    }
}
