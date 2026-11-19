//! Picks a desktop entry with wofi's drun mode and starts it through GIO.

use std::ffi::OsString;
use std::io;
use std::process::{Command, Stdio};

use gtk4::gio::{self, prelude::*};
use gtk4::glib;

use crate::command::Invocation;
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

/// A desktop entry picked, and the action picked from it, if any.
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

    /// What starting the entry runs, and the classes its windows may have:
    /// StartupWMClass, the entry's name and the program's; None for an action.
    pub fn started(&self) -> Option<(Invocation, Vec<String>)> {
        if self.action.is_some() {
            return None;
        }
        let exec = self.info.commandline()?;
        let started = Invocation::from_desktop_exec(&exec.to_string_lossy(), self.info.boolean("Terminal"))?;
        let mut classes = started.classes();
        classes.extend(self.info.startup_wm_class().map(String::from));
        classes.extend(self.info.id().and_then(|id| id.strip_suffix(".desktop").map(str::to_owned)));
        Some((started, classes))
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
