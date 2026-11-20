//! Picks a desktop entry with wofi's drun mode and starts it through GIO;
//! reads what entries and bindings start.

use std::ffi::OsString;
use std::io;
use std::process::{Command, Stdio};

use gtk4::gio::{self, prelude::*};
use gtk4::glib;

use crate::command::{self, Invocation};
use crate::entries::Candidate;
use crate::selection::{self, Selection};

/// What wofi printed for the entry picked; None when nothing was.
pub fn pick(wofi_args: &[OsString]) -> io::Result<Option<Selection>> {
    let output = Command::new("wofi")
        .args(["--show", "drun", "-D", "drun-print_desktop_file=true"])
        .args(wofi_args)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    let line = String::from_utf8_lossy(&output.stdout);
    Ok(selection::parse(&line, |path| gio_unix::DesktopAppInfo::from_filename(path).is_some()))
}

/// A desktop entry, and the action picked from it, if any.
pub struct Entry {
    info: gio_unix::DesktopAppInfo,
    action: Option<String>,
}

impl Entry {
    pub fn open(selection: &Selection) -> Result<Self, glib::Error> {
        let info = gio_unix::DesktopAppInfo::from_filename(&selection.path).ok_or_else(|| {
            glib::Error::new(gio::IOErrorEnum::NotFound, &format!("{}: not a desktop entry", selection.path))
        })?;
        Ok(Self { info, action: selection.action.clone() })
    }

    /// The desktop entry with this ID.
    pub fn by_id(id: &str) -> Option<Self> {
        Some(Self { info: find(id)?, action: None })
    }

    /// What starting the entry runs; None for an action.
    pub fn started(&self) -> Option<Invocation> {
        self.action.is_none().then(|| invocation(&self.info)).flatten()
    }

    /// The classes the entry's windows may have.
    pub fn classes(&self) -> Vec<String> {
        classes(&self.info)
    }

    /// The entry's ID without ".desktop".
    pub fn id(&self) -> Option<String> {
        id(&self.info)
    }

    /// Starts the entry, or its action.
    pub fn start(&self) -> Result<(), glib::Error> {
        let context = gio::AppLaunchContext::new();
        if self.info.boolean("PrefersNonDefaultGPU") {
            context.setenv("DRI_PRIME", "1");
        }
        if let Some(action) = &self.action {
            self.info.launch_action(action, Some(&context));
            Ok(())
        } else if self.info.boolean("DBusActivatable") {
            launch_and_wait(&self.info, &context)
        } else {
            self.info.launch_uris(&[], Some(&context))
        }
    }
}

/// What a binding's command starts: for `hyprbsd-keyhint start id` or
/// `gtk-launch id`, the entry's Exec.
pub fn starts(command: &str) -> Option<Invocation> {
    match command::launched_entry(command) {
        Some(id) => invocation(&find(&id)?),
        None => Invocation::from_command(command),
    }
}

/// The name of the desktop entry with this ID.
pub fn name(id: &str) -> Option<String> {
    find(id).map(|info| info.name().to_string())
}

/// The desktop entry with this ID, given with or without ".desktop".
fn find(id: &str) -> Option<gio_unix::DesktopAppInfo> {
    if id.ends_with(".desktop") {
        gio_unix::DesktopAppInfo::new(id)
    } else {
        gio_unix::DesktopAppInfo::new(&format!("{id}.desktop"))
    }
}

/// The desktop entries shown in menus.
pub fn candidates() -> Vec<Candidate> {
    gio::AppInfo::all()
        .into_iter()
        .filter(|info| info.should_show())
        .filter_map(|info| info.downcast::<gio_unix::DesktopAppInfo>().ok())
        .filter_map(|info| {
            Some(Candidate {
                id: id(&info)?,
                name: info.name().to_string(),
                wm_class: info.startup_wm_class().map(String::from),
                program: invocation(&info).and_then(|started| started.program().map(str::to_owned)),
                classes: classes(&info),
            })
        })
        .collect()
}

/// What the entry's Exec runs, in a terminal when it says Terminal=true.
fn invocation(info: &gio_unix::DesktopAppInfo) -> Option<Invocation> {
    let exec = info.commandline()?;
    Invocation::from_desktop_exec(&exec.to_string_lossy(), info.boolean("Terminal"))
}

/// The classes the entry's windows may have: the program's (the terminal's
/// for Terminal=true), StartupWMClass and the entry's ID.
fn classes(info: &gio_unix::DesktopAppInfo) -> Vec<String> {
    let mut classes = invocation(info).map(|started| started.classes()).unwrap_or_default();
    classes.extend(info.startup_wm_class().map(String::from));
    classes.extend(id(info));
    classes
}

fn id(info: &gio_unix::DesktopAppInfo) -> Option<String> {
    info.id().and_then(|id| id.strip_suffix(".desktop").map(str::to_owned))
}

fn launch_and_wait(info: &gio_unix::DesktopAppInfo, context: &gio::AppLaunchContext) -> Result<(), glib::Error> {
    let main = glib::MainLoop::new(None, false);
    let result = std::rc::Rc::new(std::cell::RefCell::new(Ok(())));
    let (done, stored) = (main.clone(), result.clone());
    info.launch_uris_async(&[], Some(context), None::<&gio::Cancellable>, move |launched| {
        *stored.borrow_mut() = launched;
        done.quit();
    });
    main.run();
    result.replace(Ok(()))
}
