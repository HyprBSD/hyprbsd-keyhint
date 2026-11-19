//! The card: "Open with:" and the keys, inside the bottom-right corner of
//! the program's window, or of the monitor when there's no window to sit in.
//! One at a time: a newer launch's card replaces it, an older one or one
//! asked for before a hide never shows, and a late request only gives its
//! own card on screen its window. Takes no keyboard focus, lets clicks
//! through, follows its window and closes itself.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::hyprland;
use crate::place::{self, Target};

const APP_ID: &str = "org.hyprbsd.keyhint";
const SHOWN: std::time::Duration = std::time::Duration::from_millis(3500);
const FADE: std::time::Duration = std::time::Duration::from_millis(200);
const FOLLOW: std::time::Duration = std::time::Duration::from_millis(150);
/// From the monitor's edges, clear of panels.
const MARGIN: i32 = 24;
/// From the window's edges.
const INSET: i32 = 16;

const STYLE: &str = "
window.keyhint { background: none; }
.card {
    background: #14161b;
    border: 2px solid #ef6f3c;
    border-radius: 6px;
    padding: 12px 16px;
    font-family: \"Noto Sans\";
    transition: opacity 200ms ease-in;
    animation: keyhint-in 150ms ease-out;
}
.card.leaving { opacity: 0; }
@keyframes keyhint-in { from { opacity: 0; } to { opacity: 1; } }
.lead { color: #8b919c; font-size: 11pt; }
.plus { color: #8b919c; font-size: 13pt; }
.key {
    background: #2b2f38;
    color: #e4e6eb;
    border: 1px solid #4b505c;
    border-radius: 6px;
    padding: 4px 10px;
    font-size: 15pt;
    font-weight: bold;
}
";

/// A card: who asked for it and when its launch started (nanoseconds since
/// the epoch), whether it's a window found late for a card already asked
/// for, its keys, the program's window (Hyprland's address) and the monitor
/// the launch came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub id: String,
    pub asked: u128,
    pub late: bool,
    pub caps: Vec<String>,
    pub window: Option<String>,
    pub origin: Option<String>,
}

impl Request {
    /// As arguments for the running instance: "new" or "late", id, asked,
    /// window, origin ("" for none), then the keys.
    pub fn to_args(&self) -> Vec<String> {
        let kind = if self.late { "late" } else { "new" };
        let mut args = vec![
            kind.to_owned(),
            self.id.clone(),
            self.asked.to_string(),
            self.window.clone().unwrap_or_default(),
            self.origin.clone().unwrap_or_default(),
        ];
        args.extend(self.caps.iter().cloned());
        args
    }

    /// From those arguments; None without a kind, an id, a time and a key
    /// (no card).
    pub fn from_args(args: &[String]) -> Option<Self> {
        let [kind, id, asked, window, origin, caps @ ..] = args else { return None };
        let late = match kind.as_str() {
            "new" => false,
            "late" => true,
            _ => return None,
        };
        let given = |value: &String| (!value.is_empty()).then(|| value.clone());
        (!id.is_empty() && !caps.is_empty()).then_some(())?;
        Some(Self {
            id: id.clone(),
            asked: asked.parse().ok()?,
            late,
            caps: caps.to_vec(),
            window: given(window),
            origin: given(origin),
        })
    }
}

/// Whether a card asked for at `asked` came too late to show: no later than
/// the newest card shown or the last hide.
fn superseded(asked: u128, newest: Option<u128>) -> bool {
    newest.is_some_and(|newest| asked <= newest)
}

/// The newest card's launch time, or the last hide's, kept in the session's
/// runtime directory for the card's next process.
fn newest() -> Option<u128> {
    std::fs::read_to_string(newest_file()?).ok()?.trim().parse().ok()
}

fn set_newest(time: u128) {
    let Some(path) = newest_file() else { return };
    let partial = path.with_extension("new");
    if std::fs::write(&partial, time.to_string()).is_ok() {
        let _ = std::fs::rename(&partial, &path);
    }
}

fn newest_file() -> Option<std::path::PathBuf> {
    Some(std::path::PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?).join("hyprbsd-keyhint.newest"))
}

/// Nanoseconds since the epoch.
pub fn now() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|since| since.as_nanos()).unwrap_or_default()
}

/// The card on screen and the window it follows.
struct Shown {
    id: String,
    window: glib::WeakRef<gtk4::ApplicationWindow>,
    target: Rc<RefCell<Option<String>>>,
}

thread_local! {
    static SHOWN_CARD: RefCell<Option<Shown>> = const { RefCell::new(None) };
}

/// Shows a new card in place of any other, unless it was asked for no later
/// than the newest card or the last hide; gives a late request's window to
/// the card on screen when it's that request's card, and does nothing
/// otherwise; closes the card when `request` is None. Hands over to the
/// instance already running, if any.
pub fn run(request: Option<&Request>) -> glib::ExitCode {
    let app =
        gtk4::Application::builder().application_id(APP_ID).flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE).build();
    app.connect_startup(|_| load_style());
    app.connect_command_line(|app, line| {
        let args: Vec<String> = line.arguments().iter().skip(1).map(|arg| arg.to_string_lossy().into_owned()).collect();
        match Request::from_args(&args) {
            Some(request) if request.late => update(&request),
            Some(request) if superseded(request.asked, newest()) => {}
            Some(request) => {
                set_newest(request.asked);
                close_all(app);
                show(app, request);
            }
            None => {
                set_newest(now());
                close_all(app);
            }
        }
        glib::ExitCode::SUCCESS
    });
    let args: Vec<String> = std::iter::once("hyprbsd-keyhint".to_owned())
        .chain(request.map(Request::to_args).unwrap_or_default())
        .collect();
    app.run_with_args(&args)
}

fn close_all(app: &gtk4::Application) {
    SHOWN_CARD.with_borrow_mut(|shown| *shown = None);
    for window in app.windows() {
        window.close();
    }
}

/// Gives the card on screen the request's window when the request is that
/// card's and the card is still up.
fn update(request: &Request) {
    SHOWN_CARD.with_borrow(|shown| {
        if let Some(shown) = shown.as_ref().filter(|shown| shown.id == request.id)
            && shown.window.upgrade().is_some()
            && request.window.is_some()
        {
            shown.target.replace(request.window.clone());
        }
    });
}

fn load_style() {
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(STYLE);
    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(&display, &provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
}

fn show(app: &gtk4::Application, request: Request) {
    let window = gtk4::ApplicationWindow::new(app);
    window.add_css_class("keyhint");
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_namespace(Some("hyprbsd-keyhint"));
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_anchor(Edge::Bottom, true);
    window.set_anchor(Edge::Right, true);
    window.connect_map(|window| {
        if let Some(surface) = window.surface() {
            surface.set_input_region(Some(&gtk4::cairo::Region::create()));
        }
    });

    let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    card.add_css_class("card");
    let lead = gtk4::Label::new(Some("Open with:"));
    lead.add_css_class("lead");
    card.append(&lead);
    for (i, cap) in request.caps.iter().enumerate() {
        if i > 0 {
            let plus = gtk4::Label::new(Some("+"));
            plus.add_css_class("plus");
            card.append(&plus);
        }
        let key = gtk4::Label::new(Some(cap));
        key.add_css_class("key");
        card.append(&key);
    }
    window.set_child(Some(&card));
    let (_, width, _, _) = card.measure(gtk4::Orientation::Horizontal, -1);
    let (_, height, _, _) = card.measure(gtk4::Orientation::Vertical, width);
    let size = (width, height);

    let found = request.window.as_deref().and_then(|address| {
        let (clients, monitors) = hyprland::windows_and_monitors()?;
        place::target(&clients, &monitors, address)
    });
    put(&window, found.as_ref(), request.origin.as_deref(), size);
    window.present();

    let weak = window.downgrade();
    glib::timeout_add_local_once(SHOWN, move || {
        if let Some(window) = weak.upgrade() {
            card.add_css_class("leaving");
            glib::timeout_add_local_once(FADE, move || window.close());
        }
    });
    let target = Rc::new(RefCell::new(request.window));
    SHOWN_CARD.with_borrow_mut(|shown| {
        *shown = Some(Shown { id: request.id, window: window.downgrade(), target: target.clone() });
    });
    follow(window.downgrade(), target, request.origin, size);
}

/// Every FOLLOW, puts the card at its window's current place, and closes it
/// when the window has gone. A failed request leaves the card where it is.
fn follow(
    weak: glib::WeakRef<gtk4::ApplicationWindow>,
    target: Rc<RefCell<Option<String>>>,
    origin: Option<String>,
    size: (i32, i32),
) {
    glib::MainContext::default().spawn_local(async move {
        loop {
            glib::timeout_future(FOLLOW).await;
            let Some(address) = target.borrow().clone() else {
                if weak.upgrade().is_none() {
                    return;
                }
                continue;
            };
            let reply = gio::spawn_blocking(hyprland::windows_and_monitors).await;
            let Some(window) = weak.upgrade() else { return };
            let Ok(Some((clients, monitors))) = reply else { continue };
            match place::target(&clients, &monitors, &address) {
                Some(found) => put(&window, Some(&found), origin.as_deref(), size),
                None => {
                    window.close();
                    return;
                }
            }
        }
    });
}

/// Inside the window's bottom-right corner over the whole monitor when the
/// card fits there and that monitor is found; else in the corner of the
/// window's monitor, or the origin, or the monitor the card is on, clear of
/// panels.
fn put(window: &gtk4::ApplicationWindow, target: Option<&Target>, origin: Option<&str>, size: (i32, i32)) {
    if let Some(target) = target
        && let Some((right, bottom)) = place::margins(target, size, INSET)
        && on_monitor(window, &target.monitor)
    {
        window.set_exclusive_zone(-1);
        window.set_margin(Edge::Right, right);
        window.set_margin(Edge::Bottom, bottom);
        return;
    }
    for name in [target.map(|target| target.monitor.as_str()), origin].into_iter().flatten() {
        if on_monitor(window, name) {
            break;
        }
    }
    window.set_exclusive_zone(0);
    window.set_margin(Edge::Right, MARGIN);
    window.set_margin(Edge::Bottom, MARGIN);
}

/// Puts the card on the monitor whose connector is `name`; false when no
/// monitor has that connector.
fn on_monitor(window: &gtk4::ApplicationWindow, name: &str) -> bool {
    let Some(display) = gdk::Display::default() else { return false };
    let monitors = display.monitors();
    let Some(monitor) = (0..monitors.n_items())
        .filter_map(|i| monitors.item(i).and_downcast::<gdk::Monitor>())
        .find(|monitor| monitor.connector().as_deref() == Some(name))
    else {
        return false;
    };
    if window.monitor().as_ref() != Some(&monitor) {
        window.set_monitor(Some(&monitor));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(window: Option<&str>, origin: Option<&str>) -> Request {
        Request {
            id: "4242-1".into(),
            asked: 1_795_000_000_000_000_000,
            late: false,
            caps: vec!["SUPER".into(), "E".into()],
            window: window.map(Into::into),
            origin: origin.map(Into::into),
        }
    }

    #[test]
    fn a_request_should_come_back_from_its_arguments() {
        let sent = request(Some("0x5"), Some("DP-3"));
        assert_eq!(Request::from_args(&sent.to_args()), Some(sent));
    }

    #[test]
    fn a_request_without_window_or_origin_should_come_back_too() {
        let sent = request(None, None);
        assert_eq!(Request::from_args(&sent.to_args()), Some(sent));
    }

    #[test]
    fn a_key_alone_should_come_back_too() {
        let sent = Request { caps: vec!["PRINT".into()], ..request(None, None) };
        assert_eq!(Request::from_args(&sent.to_args()), Some(sent));
    }

    #[test]
    fn no_arguments_should_be_no_card() {
        assert_eq!(Request::from_args(&[]), None);
    }

    #[test]
    fn no_keys_should_be_no_card() {
        assert_eq!(
            Request::from_args(&["new".into(), "4242-1".into(), "1".into(), String::new(), String::new()]),
            None
        );
    }

    #[test]
    fn no_id_should_be_no_card() {
        assert_eq!(
            Request::from_args(&[
                "new".into(),
                String::new(),
                "1".into(),
                String::new(),
                String::new(),
                "PRINT".into()
            ]),
            None
        );
    }

    #[test]
    fn a_late_request_should_come_back_late() {
        let sent = Request { late: true, ..request(Some("0x5"), None) };
        assert_eq!(Request::from_args(&sent.to_args()), Some(sent));
    }

    #[test]
    fn an_unknown_kind_should_be_no_card() {
        assert_eq!(
            Request::from_args(&["soon".into(), "4242-1".into(), "1".into(), String::new(), String::new(), "E".into()]),
            None
        );
    }

    #[test]
    fn a_time_that_isnt_a_number_should_be_no_card() {
        assert_eq!(
            Request::from_args(&[
                "new".into(),
                "4242-1".into(),
                "soon".into(),
                String::new(),
                String::new(),
                "E".into()
            ]),
            None
        );
    }

    #[test]
    fn a_card_should_show_when_nothing_came_before() {
        assert!(!superseded(5, None));
    }

    #[test]
    fn a_card_asked_for_after_the_newest_should_show() {
        assert!(!superseded(6, Some(5)));
    }

    #[test]
    fn a_card_asked_for_before_the_newest_or_the_last_hide_should_not() {
        assert!(superseded(4, Some(5)));
    }

    #[test]
    fn a_card_asked_for_at_the_same_time_should_not() {
        assert!(superseded(5, Some(5)));
    }
}
