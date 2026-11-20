#!/usr/bin/env python3.12
#
# A stand-in for Hyprland's sockets in the directory given. Its request
# socket answers j/configerrors with the lines of the file $FAKE_ERRORS
# (none while it's missing), the hyprbsd_shortcuts() repl with the file
# $FAKE_SHORTCUTS, and j/clients, j/monitors, j/activewindow and j/binds
# with the files $FAKE_CLIENTS, $FAKE_MONITORS, $FAKE_ACTIVE and
# $FAKE_BINDS; it logs each request to $FAKE_LOG. Its event socket sends
# each client the lines added to the file $FAKE_EVENTS after it connected.
#
#   fake-hyprland.py directory
#
# SPDX-License-Identifier: BSD-2-Clause

import json
import os
import socketserver
import sys
import threading
import time

REPL = "/repl return hyprbsd_shortcuts and hyprbsd_shortcuts()"
FILES = {"j/clients": ("FAKE_CLIENTS", "[]"), "j/monitors": ("FAKE_MONITORS", "[]"),
         "j/activewindow": ("FAKE_ACTIVE", "{}"), "j/binds": ("FAKE_BINDS", "[]")}


def read(variable, missing):
    try:
        with open(os.environ[variable]) as f:
            return f.read()
    except FileNotFoundError:
        return missing


class Requests(socketserver.StreamRequestHandler):
    def handle(self):
        request = self.request.recv(65536).decode()
        with open(os.environ["FAKE_LOG"], "a") as log:
            log.write(request + "\n")
        if request == "j/configerrors":
            errors = read("FAKE_ERRORS", "")
            reply = json.dumps(errors.splitlines() if errors else [""])
        elif request == REPL:
            reply = read("FAKE_SHORTCUTS", "")
        elif request in FILES:
            reply = read(*FILES[request])
        else:
            reply = "unknown request"
        self.request.sendall(reply.encode())


class Events(socketserver.StreamRequestHandler):
    def handle(self):
        path = os.environ["FAKE_EVENTS"]
        offset = os.path.getsize(path) if os.path.exists(path) else 0
        while True:
            if os.path.exists(path) and os.path.getsize(path) > offset:
                with open(path, "rb") as f:
                    f.seek(offset)
                    data = f.read()
                offset += len(data)
                try:
                    self.request.sendall(data)
                except OSError:
                    return
            time.sleep(0.05)


class Server(socketserver.ThreadingMixIn, socketserver.UnixStreamServer):
    daemon_threads = True


directory = sys.argv[1]
events = Server(os.path.join(directory, ".socket2.sock"), Events)
threading.Thread(target=events.serve_forever, daemon=True).start()
Server(os.path.join(directory, ".socket.sock"), Requests).serve_forever()
