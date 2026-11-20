# hyprbsd-keyhint

![CI](https://github.com/HyprBSD/hyprbsd-keyhint/actions/workflows/check.yml/badge.svg?branch=main)
![FreeBSD](https://img.shields.io/badge/FreeBSD-ab2b28?logo=freebsd)
![License](https://img.shields.io/github/license/HyprBSD/hyprbsd-keyhint)
[![Buy Me a Coffee](https://img.shields.io/badge/Buy%20Me%20a%20Coffee-support-ffdd00?logo=buymeacoffee)](https://buymeacoffee.com/sheridans)

Teaches HyprBSD's key bindings. When you open a program from the launcher, the HyprBSD menu or
a button on the bar, and a key binding starts it too, a card shows those keys for a few seconds
inside the bottom-right corner of the program's window. It isn't a notification: it never takes
your keyboard, clicks go through it, and it closes by itself. Opening the program with its keys
shows nothing.

![The card inside a Firefox window: Open with: SUPER + W](docs/screenshot.png)

It follows the window if it moves, and goes when the window closes. With no window of the
program's (a menu, a panel), the card shows in the corner of the monitor you were on. It knows
your own bindings as well as HyprBSD's.

It also gives a program keys of your own: HyprBSD's window menu (Super+Shift+W) asks it which
program a window is, then shows a dialog for the keys, which refuses keys another binding has.

It's part of [HyprBSD](https://hyprbsd.org).

## Install

**On HyprBSD** it's installed with the desktop. The launcher (Super+R, Super+Space), the
HyprBSD menu and the bar use it, and the menu's Toggles, Shortcut hints turns it off.

**On FreeBSD, from source:**

```
pkg install rust pkgconf gtk4 gtk4-layer-shell
make
make install
```

It installs `/usr/local/bin/hyprbsd-keyhint`; `make install PREFIX=/opt/keyhint` puts it
elsewhere, and `DESTDIR` installs into a staging directory. It reads the bindings from
`hyprbsd_shortcuts()`, which HyprBSD's Hyprland config defines, so outside HyprBSD it shows
nothing until your config has that function.

## Usage

- **`hyprbsd-keyhint launch [wofi option ...]`** opens wofi's app list, starts the program you
  pick and shows its keys.
- **`hyprbsd-keyhint show command [arg ...]`** shows the keys of the binding that runs that
  command, for menus and bars that start it themselves. It prints `ready` once it's watching for
  the program's window, so start the command after that.
- **`hyprbsd-keyhint hide`** closes the card.
- **`hyprbsd-keyhint entry address`** prints the ID and name of the desktop entry the window at
  that Hyprland address is from: the one `launch` or `start` opened it from, else the one its
  class names. It exits 1 when it can't tell.
- **`hyprbsd-keyhint bind id`** asks for keys to start desktop entry `id` (Super and a key) and
  prints them as Hyprland's bind takes them, such as `SUPER + ALT + G`. It exits 1 on Escape or a
  minute after it opens, 2 when there's no such entry, and 3 when there's no display to show it
  on. While it's open, every key goes to it, the compositor's own bindings included.
- **`hyprbsd-keyhint start id`** starts desktop entry `id` with no card, as keys saved from the
  window menu do, so `entry` knows its window.

No card shows while `~/.config/hyprbsd/keyhint-off` exists.

## Building

You need Rust (FreeBSD's `rust` package), pkgconf, GTK 4, gtk4-layer-shell and FreeBSD's make;
`make dist` needs `git` too:

```
make            # target/release/hyprbsd-keyhint
make check      # rustfmt's check, the tests, clippy and the session test
make deny       # cargo-deny: the crates' licences, advisories and sources (deny.toml)
```

[CONTRIBUTING.md](CONTRIBUTING.md) has the tests and how a release is made. Report security
problems privately: see [SECURITY.md](SECURITY.md).

## Licence

BSD-2-Clause; see `LICENSE`.
