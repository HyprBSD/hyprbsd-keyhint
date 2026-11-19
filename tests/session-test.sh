#!/bin/sh
#
# hyprbsd-keyhint in a headless sway with two outputs (HEADLESS-1 at 0,0,
# 1280x720; HEADLESS-2 at 1280,0, 1920x1080, a 40-pixel bar along its
# bottom), a private D-Bus session, a stand-in for Hyprland's sockets
# (tests/fake-hyprland.py) and one for wofi that picks $FAKE_PICK.
# Desktop entries start once with the arguments, terminal, action and
# DRI_PRIME GIO gives them. The card shows only for what a binding starts,
# never while the config has errors or the hints are off: inside the bottom-
# right corner of the program's window, on that window's monitor and
# following it, or in the corner of the launch's monitor when no window of
# the program's opens in time, and closes by itself or with its window.
#
#   tests/session-test.sh hyprbsd-keyhint
#
# Needs sway, grim, jq, python3.12 with PyGObject and dbus-run-session.
#
# SPDX-License-Identifier: BSD-2-Clause

set -u
[ $# -eq 1 ] || { echo "usage: $0 hyprbsd-keyhint" >&2; exit 64; }
bin=$(realpath "$1") || exit 1
here=$(cd "$(dirname "$0")" && pwd)

# The run's files, its own D-Bus session reading their service files.
# Sockets need a path under 104 bytes.
if [ -z "${KEYHINT_TEST_TMP:-}" ]; then
	tmp=$(mktemp -d /tmp/keyhint.XXXXXX) && [ -d "$tmp" ] || exit 1
	chmod 0700 "$tmp"
	mkdir -p "$tmp/data/dbus-1/services"
	env KEYHINT_TEST_TMP="$tmp" XDG_DATA_HOME="$tmp/data" XDG_DATA_DIRS="$tmp/data" \
		dbus-run-session -- "$0" "$@"
	status=$?
	rm -rf "$tmp"
	exit $status
fi
tmp=$KEYHINT_TEST_TMP
failures=0
pids=
trap 'kill $pids 2>/dev/null; pkill -f "$bin card" 2>/dev/null' EXIT
trap 'exit 1' INT TERM HUP PIPE

check() {
	if eval "$2"; then
		echo "ok    $1"
	else
		echo "FAIL  $1"
		failures=$((failures + 1))
	fi
}

hypr=$tmp/run/hypr/FAKE
mkdir -p "$tmp/bin" "$tmp/home/.config/hyprbsd" "$tmp/data/applications" "$tmp/data/dbus-1/services" "$hypr"
export HOME="$tmp/home" XDG_RUNTIME_DIR="$tmp/run" XDG_DATA_HOME="$tmp/data" XDG_DATA_DIRS="$tmp/data"
export HYPRLAND_INSTANCE_SIGNATURE=FAKE PATH="$tmp/bin:$PATH" GSK_RENDERER=cairo GDK_DEBUG=no-portals GTK_A11Y=none
export RECORD="$tmp/record" FAKE_LOG="$tmp/requests" FAKE_SHORTCUTS="$tmp/shortcuts" FAKE_ERRORS="$tmp/errors"
export FAKE_CLIENTS="$tmp/clients" FAKE_MONITORS="$tmp/monitors" FAKE_ACTIVE="$tmp/active" FAKE_EVENTS="$tmp/events"
unset WAYLAND_DISPLAY DISPLAY

# record: each start as one line, its arguments in brackets, then DRI_PRIME.
printf '#!/bin/sh\nline=\nfor a; do line="$line[$a]"; done\necho "$line DRI_PRIME=${DRI_PRIME-unset}" >>"$RECORD"\n' \
	>"$tmp/bin/record"
# The terminal GIO runs for Terminal=true entries.
printf '#!/bin/sh\nline=terminal\nfor a; do line="$line[$a]"; done\necho "$line" >>"$RECORD"\n' \
	>"$tmp/bin/xdg-terminal-exec"
printf '#!/bin/sh\n[ -n "${FAKE_PICK:-}" ] || exit 1\necho "$FAKE_PICK"\n' >"$tmp/bin/wofi"
# window address class x y width height monitor: a program's window as
# Hyprland reports it, added to the window list, then announced.
cat >"$tmp/bin/window" <<'SH'
#!/bin/sh
echo "window $1" >>"$RECORD"
jq -c --arg a "0x$1" --arg c "$2" --argjson x "$3" --argjson y "$4" --argjson w "$5" --argjson h "$6" \
	--argjson m "$7" '. + [{address: $a, class: $c, at: [$x, $y], size: [$w, $h], monitor: $m}]' \
	"$FAKE_CLIENTS" >"$FAKE_CLIENTS.new" && mv "$FAKE_CLIENTS.new" "$FAKE_CLIENTS"
printf 'openwindow>>%s,1,%s,%s\n' "$1" "$2" "$2" >>"$FAKE_EVENTS"
SH
chmod +x "$tmp/bin/record" "$tmp/bin/xdg-terminal-exec" "$tmp/bin/wofi" "$tmp/bin/window"
: >"$FAKE_EVENTS"

entry() {
	{
		printf '[Desktop Entry]\nType=Application\nName=%s\n' "$1"
		shift
		printf '%s\n' "$@"
	} >"$tmp/data/applications/$name.desktop"
}
name=plain entry Plain "Exec=record plain %U"
name=spaces entry Spaces "Exec=record \"two words\" 'single'"
name=codes entry Codes "Exec=record %f --level=100%% %U"
name=term entry Term "Exec=record term" "Terminal=true"
name=actions entry Actions "Exec=record main" "Actions=second;" "" "[Desktop Action second]" "Name=Second" \
	"Exec=record second"
name=prime entry Prime "Exec=record prime" "PrefersNonDefaultGPU=true"
name=org.hyprbsd.KeyhintTest entry DBus "Exec=record dbus-exec" "DBusActivatable=true"
name=org.hyprbsd.KeyhintSlow entry Slow "Exec=record slow" "DBusActivatable=true"
name=win entry Win "Exec=window b1 galculator 1480 100 800 500 1" "StartupWMClass=galculator"
apps=$tmp/data/applications

# service.py id [release]: a D-Bus activated program; records its
# activation, once the file release exists if given, and quits.
cat >"$tmp/service.py" <<'PY'
import os
import sys
import time
import gi
gi.require_version("Gio", "2.0")
from gi.repository import Gio
app = Gio.Application(application_id=sys.argv[1], flags=Gio.ApplicationFlags.IS_SERVICE)
def activate(app):
    for _ in range(200):
        if len(sys.argv) < 3 or os.path.exists(sys.argv[2]):
            break
        time.sleep(0.05)
    with open(os.environ["RECORD"], "a") as record:
        record.write("dbus activated\n")
    app.quit()
app.connect("activate", activate)
app.run([sys.argv[0]])
PY
printf '[D-BUS Service]\nName=org.hyprbsd.KeyhintTest\nExec=/usr/bin/env RECORD=%s python3.12 %s org.hyprbsd.KeyhintTest\n' \
	"$RECORD" "$tmp/service.py" >"$tmp/data/dbus-1/services/org.hyprbsd.KeyhintTest.service"
printf '[D-BUS Service]\nName=org.hyprbsd.KeyhintSlow\nExec=/usr/bin/env RECORD=%s python3.12 %s org.hyprbsd.KeyhintSlow %s\n' \
	"$RECORD" "$tmp/service.py" "$tmp/release-slow" >"$tmp/data/dbus-1/services/org.hyprbsd.KeyhintSlow.service"

printf 'hyprbsd-shortcuts 1\n64\tReturn\tfoot\n72\tC\tgalculator\n64\tP\trecord plain\n64\tA\trecord second\n%s\n%s\n%s\n' \
	"$(printf '64\tE\tthunar')" "$(printf '64\tG\twindow b1 galculator 1480 100 800 500 1')" \
	"$(printf '64\tS\trecord slow')" >"$FAKE_SHORTCUTS"

# The monitors as Hyprland reports them, $1 in focus, the second named $2
# (HEADLESS-2 unless given).
monitors() {
	jq -n -c --arg f "$1" --arg second "${2:-HEADLESS-2}" '[
		{id: 0, name: "HEADLESS-1", x: 0, y: 0, width: 1280, height: 720, scale: 1.0, transform: 0},
		{id: 1, name: $second, x: 1280, y: 0, width: 1920, height: 1080, scale: 1.0, transform: 0}
	] | map(. + {focused: (.name == $f)})' >"$FAKE_MONITORS"
}

python3.12 "$here/fake-hyprland.py" "$hypr" &
pids="$pids $!"
printf 'bar {\n  output HEADLESS-2\n  position bottom\n  height 40\n  mode dock\n  status_command while :; do echo .; sleep 60; done\n}\n' \
	>"$tmp/sway.config"
WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 sway -c "$tmp/sway.config" >"$tmp/sway.log" 2>&1 &
pids="$pids $!"
i=0
while [ $i -lt 50 ] && [ ! -S "$XDG_RUNTIME_DIR/wayland-1" ]; do
	sleep 0.2
	i=$((i + 1))
done
[ -S "$XDG_RUNTIME_DIR/wayland-1" ] || { echo "sway didn't start: $(tail -n 1 "$tmp/sway.log")"; exit 1; }
export WAYLAND_DISPLAY=wayland-1
SWAYSOCK=$(ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock) swaymsg create_output >/dev/null
sleep 1

# Whether the card's orange border is at x,y.
orange_at() {
	[ "$(grim -g "$1,$2 1x1" -t ppm - | tail -c 3 | od -An -tx1 | tr -d ' \n')" = ef6f3c ]
}
# The card in HEADLESS-1's corner, HEADLESS-2's corner (above its bar), or
# inside the bottom-right corner of a window at 1480,100, 800x500.
card_shown() { orange_at 1255 663; }
in_second_corner() { orange_at 3175 996; }
at_window() { orange_at 2263 564; }
cards() { pgrep -f "$bin card" | wc -l | tr -d ' '; }
# start command ...: runs it in the background long enough for a card to show.
start() {
	"$@" >/dev/null &
	card=$!
	sleep 1.5
}
# The status of the last start's command, once it has finished.
settle() {
	wait $card
}
# show command ...: hyprbsd-keyhint show in the background, up to 2 s for it
# to say it's ready.
show() {
	: >"$tmp/ready"
	"$bin" show "$@" >"$tmp/ready" &
	card=$!
	_i=0
	while [ "$(cat "$tmp/ready")" != ready ] && [ $_i -lt 20 ]; do
		sleep 0.1
		_i=$((_i + 1))
	done
}
reset() {
	: >"$RECORD"
	: >"$FAKE_LOG"
	rm -f "$FAKE_ERRORS" "$HOME/.config/hyprbsd/keyhint-off"
	echo '[]' >"$FAKE_CLIENTS"
	echo '{}' >"$FAKE_ACTIVE"
	monitors HEADLESS-1
	"$bin" hide
	_i=0
	while [ "$(cards)" -gt 0 ] && [ $_i -lt 20 ]; do
		sleep 0.1
		_i=$((_i + 1))
	done
}
requests() { tr '\n' ' ' <"$FAKE_LOG"; }
repl='/repl return hyprbsd_shortcuts and hyprbsd_shortcuts()'
recorded() { [ "$(cat "$RECORD")" = "$1" ]; }
move() {
	jq -c --arg a "$1" --argjson x "$2" --argjson y "$3" 'map(if .address == $a then .at = [$x, $y] else . end)' \
		"$FAKE_CLIENTS" >"$FAKE_CLIENTS.new" && mv "$FAKE_CLIENTS.new" "$FAKE_CLIENTS"
}
remove() {
	jq -c --arg a "$1" 'map(select(.address != $a))' "$FAKE_CLIENTS" >"$FAKE_CLIENTS.new" &&
		mv "$FAKE_CLIENTS.new" "$FAKE_CLIENTS"
}

reset
start "$bin" show foot
check "show: no window, the card in the launch's monitor's corner" card_shown
check "show: asks for config errors, then the bindings" "case \"\$(requests)\" in 'j/configerrors $repl '*) true ;; *) false ;; esac"
sleep 4
check "show: the card closes by itself, and its program with it" "! card_shown && [ \"\$(cards)\" -eq 0 ]"
settle

reset
echo "custom.lua:3: unexpected symbol" >"$FAKE_ERRORS"
start "$bin" show foot
check "config errors: no card" "! card_shown"
check "config errors: the bindings aren't asked for" "[ \"\$(requests)\" = 'j/configerrors ' ]"
settle

reset
: >"$HOME/.config/hyprbsd/keyhint-off"
start "$bin" show foot
check "hints off: nothing asked, no card" "[ ! -s '$FAKE_LOG' ] && ! card_shown"
settle

reset
start "$bin" show gimp
check "a command no binding runs: no card" "! card_shown"
settle

reset
start "$bin" show foot
sleep 0.5
start "$bin" show galculator
check "a second card goes to the program showing the first" "[ \"\$(cards)\" -eq 1 ]"
sleep 1
check "the newer card gets its own time" card_shown
sleep 3
check "the newer card closes by itself, and its program with it" "! card_shown && [ \"\$(cards)\" -eq 0 ]"
settle

reset
start "$bin" show foot
"$bin" hide
sleep 0.5
check "hide closes the card" "! card_shown"
settle

reset
start env FAKE_PICK="$apps/plain.desktop" "$bin" launch
check "launch: an entry runs once, without its field codes" "recorded '[plain] DRI_PRIME=unset'"
check "launch: the card for an entry a binding starts" card_shown
settle

reset
start env FAKE_PICK="$apps/spaces.desktop" "$bin" launch
check "launch: quoted arguments stay whole" "recorded '[two words][single] DRI_PRIME=unset'"
check "launch: no card for an entry no binding starts" "! card_shown"
settle

reset
start env FAKE_PICK="$apps/codes.desktop" "$bin" launch
check "launch: field codes out, %% as %" "recorded '[--level=100%] DRI_PRIME=unset'"
settle

reset
start env FAKE_PICK="$apps/term.desktop" "$bin" launch
check "launch: Terminal=true runs through the terminal, once" "recorded 'terminal[record][term]'"
settle

reset
start env FAKE_PICK="$apps/actions.desktop second" "$bin" launch
check "launch: an action runs its own command once" "recorded '[second] DRI_PRIME=unset'"
check "launch: no card for an action" "! card_shown"
settle

reset
start env FAKE_PICK="$apps/prime.desktop" "$bin" launch
check "launch: PrefersNonDefaultGPU sets DRI_PRIME=1" "recorded '[prime] DRI_PRIME=1'"
settle

reset
start env FAKE_PICK="$apps/org.hyprbsd.KeyhintTest.desktop" "$bin" launch
check "launch: a D-Bus entry is activated over D-Bus, not run" "recorded 'dbus activated'"
settle

reset
start "$bin" launch
settle
status=$?
check "launch: nothing picked, nothing run, nothing asked" \
	"[ $status -eq 0 ] && [ ! -s '$RECORD' ] && [ ! -s '$FAKE_LOG' ]"

reset
start env FAKE_PICK="$apps/plain.desktop" HYPRLAND_INSTANCE_SIGNATURE= "$bin" launch
settle
status=$?
check "launch: no Hyprland to ask, the entry still runs" "[ $status -eq 0 ] && recorded '[plain] DRI_PRIME=unset'"

reset
start env FAKE_PICK="$apps/win.desktop" "$bin" launch
check "launch: the card inside its window, on the window's monitor" "at_window && ! card_shown"
check "launch: the program started once" "recorded 'window b1'"
settle

reset
show thunar
check "show: ready at once" "[ \"\$(cat '$tmp/ready')\" = ready ]"
window c1 steam 1300 50 900 600 1
sleep 0.2
window c2 thunar 1480 100 800 500 1
sleep 1
check "another program's window opening first is passed over" "at_window && ! orange_at 2183 614"
move 0xc2 1380 100
sleep 0.5
check "the card follows its window" "orange_at 2163 564 && ! at_window"
remove 0xc2
sleep 0.5
check "the card closes with its window" "! orange_at 2163 564 && [ \"\$(cards)\" -eq 0 ]"
settle

reset
show thunar
sleep 1.2
check "no window yet: the card in the launch's monitor's corner" card_shown
window d1 thunar 1480 100 800 500 1
sleep 0.6
check "the window opening later: the card moves to it" "at_window && ! card_shown"
settle

reset
monitors HEADLESS-2
show thunar
sleep 1.2
check "launched from the second monitor: the card in its corner, above its bar" "in_second_corner && ! card_shown"
settle

reset
show thunar
window e1 thunar 1480 100 200 80 1
sleep 1
check "a window too small for the card: its monitor's corner" "in_second_corner && ! at_window"
settle

reset
show thunar
window f1 thunar 1480 500 800 560 1
sleep 1
check "a window over the bar: the card inside it, not moved up by the bar" "orange_at 2263 1035 && ! orange_at 2263 950"
settle

reset
window a1 thunar 1480 100 800 500 1
echo '{"address": "0xa1", "class": "thunar"}' >"$FAKE_ACTIVE"
show thunar
sleep 1.2
check "the program's window already in focus: the card in it" at_window
window a3 thunar 1380 300 700 400 1
sleep 0.6
check "and a new window of the program's: the card moves to it" "orange_at 2063 664 && ! at_window"
settle

reset
window a1 thunar 1480 100 800 500 1
window a2 foot 100 100 400 300 0
echo '{"address": "0xa2", "class": "foot"}' >"$FAKE_ACTIVE"
show thunar
printf 'activewindowv2>>a1\n' >>"$FAKE_EVENTS"
sleep 1
check "the program's own window brought forward: the card in it" at_window
settle

reset
mv "$hypr/.socket2.sock" "$hypr/.socket2.away"
start env FAKE_PICK="$apps/win.desktop" "$bin" launch
check "no event socket: the program still starts, once" "recorded 'window b1'"
check "no event socket: the card in a corner" card_shown
settle
mv "$hypr/.socket2.away" "$hypr/.socket2.sock"

# A window of the program's opening after its card gave way: replaced by a
# newer card, hidden with the hints turned off, or gone after its time.
reset
show thunar
thunar=$card
sleep 1.2
show galculator
window g1 galculator 300 100 800 500 0
sleep 0.8
window t1 thunar 1480 100 800 500 1
sleep 0.6
check "a late window doesn't take back a newer card" "orange_at 1083 564 && ! at_window"
wait $thunar
settle

reset
show thunar
sleep 1.2
: >"$HOME/.config/hyprbsd/keyhint-off"
"$bin" hide
window t2 thunar 1480 100 800 500 1
sleep 0.6
check "a late window doesn't bring back a card hidden with the hints off" \
	"! at_window && ! card_shown && [ \"\$(cards)\" -eq 0 ]"
settle

reset
show thunar
_i=0
while [ "$(cards)" -eq 0 ] && [ $_i -lt 30 ]; do
	sleep 0.05
	_i=$((_i + 1))
done
while [ "$(cards)" -gt 0 ] && [ $_i -lt 100 ]; do
	sleep 0.05
	_i=$((_i + 1))
done
window t3 thunar 1480 100 800 500 1
watching=no
kill -0 $card 2>/dev/null && watching=yes
sleep 0.6
check "a late window doesn't bring back a card that had its time (still watching then: $watching)" \
	"[ $watching = yes ] && ! at_window && ! card_shown"
settle

# Before the first card shows: the hints turned off and the card hidden, or
# a newer card already up when an older launch's first card arrives (its
# watcher held back with STOP and CONT).
reset
show thunar
: >"$HOME/.config/hyprbsd/keyhint-off"
"$bin" hide
sleep 1.2
check "the hints turned off before the first card: no card" "! card_shown && [ \"\$(cards)\" -eq 0 ]"
settle

reset
show thunar
thunar=$card
kill -STOP $thunar
show galculator
window g2 galculator 300 100 800 500 0
sleep 1
newer=no
orange_at 1083 564 && newer=yes
kill -CONT $thunar
sleep 1
check "an older launch's first card doesn't replace a newer card (newer up first: $newer)" \
	"[ $newer = yes ] && orange_at 1083 564 && ! card_shown"
wait $thunar
settle

# A D-Bus program whose activation takes a while, with a newer card shown
# in the meantime.
reset
rm -f "$tmp/release-slow"
start env FAKE_PICK="$apps/org.hyprbsd.KeyhintSlow.desktop" "$bin" launch
slow=$card
show galculator
window g3 galculator 300 100 800 500 0
sleep 1
newer=no
orange_at 1083 564 && newer=yes
: >"$tmp/release-slow"
sleep 1.5
check "a launch slow to start doesn't replace a newer card (newer up first: $newer)" \
	"[ $newer = yes ] && grep -qx 'dbus activated' '$RECORD' && orange_at 1083 564 && ! card_shown"
wait $slow
settle

reset
monitors HEADLESS-1 MISSING-CONNECTOR
show thunar
window m1 thunar 1480 100 800 500 1
sleep 1
check "the window's monitor not found: the card in the launch's monitor's corner" \
	"card_shown && ! orange_at 343 204"
settle

[ "$failures" -eq 0 ]
