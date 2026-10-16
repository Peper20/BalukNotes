#!/usr/bin/env python3
"""A page screenshot or a measurement in WebKitGTK (the engine of the desktop window).

Counterpart of tools/shot.mjs for WebKit: opens the URL in a Gtk.Window inside
a headless gamescope (no window ever appears on the user's display), waits for
<html data-state="ready"> and document.fonts.ready, then saves a PNG and/or
prints the value of a JS expression.

  tools/shot-webkit.py <url> [out.png] [--size 1300x900] [--dark|--light] [--wait 300]
                       [--eval 'JS'] [--print 'JS expression']

  --dark / --light   what matchMedia("(prefers-color-scheme: ...)") answers (without them - the system one)
  --wait   extra time after rendering, ms
  --eval   run before the screenshot / measurement (e.g. open the settings)
  --print  print the JSON value of a synchronous JS expression (a promise is not awaited)

The script starts itself again under `gamescope --backend headless -W <w> -H
<h>` (the output is the size of --size) and passes the result on: gamescope
exits with 0 even when its child fails, so the child writes its exit code to a
temporary file, which the first process reads. Output of gamescope and of the
child is kept in the temporary directory and shown only on failure; the
--print value goes to stdout. The first process passes its DISPLAY on as
BALUK_ORIG_DISPLAY: the child refuses to run unless it is inside gamescope
(GAMESCOPE_WAYLAND_DISPLAY) and its DISPLAY is set and differs from that one
(an Xwayland that failed to start would leave the user's DISPLAY in place).

Needs python `gi` with WebKit2 4.1 and Gtk 3 (webkit2gtk-4.1) and gamescope.
Exit codes: 0 done; 1 the run failed (a timeout, a JS error, a page that does
not load, a child that crashed, or gamescope ran but left no result); 2 it
cannot start (no gamescope, or the refusal above). It never opens a visible
window.
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import signal
from pathlib import Path
from urllib.parse import quote

# Set in the process that runs inside gamescope: the path of the directory where it leaves its results.
GUARD = "SHOT_WEBKIT_GAMESCOPE"
# Set for the process inside gamescope: the user's DISPLAY. Inside gamescope DISPLAY must be another one.
ORIG_DISPLAY = "BALUK_ORIG_DISPLAY"
# How long the whole run may take; the page itself has 60 s (see `shoot`).
RUN_LIMIT = 120

# The window follows the system scheme (dark on the author's desktop and not overridable from here):
# --dark/--light make `matchMedia("(prefers-color-scheme: ...)")` answer for the page, which is how the client picks a theme.
SCHEME_SCRIPT = """
(() => {
  const orig = window.matchMedia.bind(window);
  window.matchMedia = (query) => {
    const m = /prefers-color-scheme:\\s*(dark|light)/.exec(query);
    if (!m) return orig(query);
    return { matches: m[1] === "SCHEME", media: query, onchange: null,
      addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {},
      dispatchEvent: () => false };
  };
})();
"""

# WebKit does not await promises in evaluate_javascript: the page is polled.
READY = 'document.documentElement.dataset.state === "ready" && document.fonts.status === "loaded"'


def parse_args() -> argparse.Namespace:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("url")
    ap.add_argument("out", nargs="?")
    ap.add_argument("--size", default="1300x900")
    ap.add_argument("--dark", action="store_true")
    ap.add_argument("--light", action="store_true")
    ap.add_argument("--wait", type=int, default=300)
    ap.add_argument("--eval", dest="eval_js")
    ap.add_argument("--print", dest="print_js")
    args = ap.parse_args()
    try:
        args.width, args.height = (int(x) for x in args.size.split("x"))
    except ValueError:
        ap.error("--size must look like 1300x900")
    return args


def in_gamescope(args: argparse.Namespace) -> int:
    """Starts this script again inside a headless gamescope and returns the child's exit code."""
    gamescope = shutil.which("gamescope")
    if gamescope is None:
        print(
            "error: gamescope is not installed; the page is shown only inside a headless gamescope "
            "(never on the user's display)",
            file=sys.stderr,
        )
        return 2
    with tempfile.TemporaryDirectory(prefix="shot-webkit-") as tmp:
        out = Path(tmp)
        env = {**os.environ, GUARD: tmp, ORIG_DISPLAY: os.environ.get("DISPLAY", "")}
        command = [
            gamescope, "--backend", "headless", "-W", str(args.width), "-H", str(args.height),
            "--", sys.executable, os.path.abspath(__file__), *sys.argv[1:],
        ]
        log = open(out / "gamescope.log", "wb")
        # Its own process group: a hung run is killed together with the gamescope's children.
        proc = subprocess.Popen(command, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            proc.wait(timeout=RUN_LIMIT)
        except subprocess.TimeoutExpired:
            print(f"error: no result within {RUN_LIMIT} s, stopped", file=sys.stderr)
        finally:
            try:
                os.killpg(proc.pid, signal.SIGKILL)  # whatever is left
            except ProcessLookupError:
                pass
            proc.wait()
        log.close()
        # gamescope exits with 0 whatever its child did: the code is in the file the child wrote.
        try:
            code = int((out / "status").read_text())
        except (OSError, ValueError):
            code = None
        if (out / "stdout").exists():
            sys.stdout.write((out / "stdout").read_text())
        if code != 0:
            if (out / "stderr").exists():
                sys.stderr.write((out / "stderr").read_text())
            if code is None:
                sys.stderr.write((out / "gamescope.log").read_text(errors="replace"))
                print("error: the child left no result (gamescope or WebKit failed)", file=sys.stderr)
                return 1
        return code


def guard_error() -> str | None:
    """Why the part inside gamescope must not open a window, or None when it may."""
    if "GAMESCOPE_WAYLAND_DISPLAY" not in os.environ:
        return "not inside gamescope"
    if ORIG_DISPLAY not in os.environ:
        return f"{ORIG_DISPLAY} is not set"
    display = os.environ.get("DISPLAY", "")
    if not display or display == os.environ[ORIG_DISPLAY]:
        return f"DISPLAY ({display or 'empty'}) is not the X server of gamescope"
    return None


def run_child(args: argparse.Namespace) -> int:
    """The part inside gamescope: its stdout and stderr and the exit code go to files of the first process."""
    out = Path(os.environ[GUARD])
    sys.stdout.flush()
    for name, fd in (("stdout", 1), ("stderr", 2)):
        os.dup2(os.open(out / name, os.O_WRONLY | os.O_CREAT | os.O_TRUNC), fd)
    code = 2  # stays 2 for a refusal ("cannot start"); the code is written whatever happens
    try:
        refusal = guard_error()
        if refusal:
            print(f"error: {refusal}, refusing to open a window", file=sys.stderr)
        else:
            # The X server of gamescope only: never the user's Wayland session.
            os.environ.pop("WAYLAND_DISPLAY", None)
            os.environ["GDK_BACKEND"] = "x11"
            code = shoot(args)
    except BaseException as err:
        code = 1
        print(f"error: {err!r}", file=sys.stderr)
    finally:
        sys.stdout.flush()
        (out / "status").write_text(str(code))
    return code


def main() -> int:
    args = parse_args()
    return run_child(args) if os.environ.get(GUARD) else in_gamescope(args)


def shoot(args: argparse.Namespace) -> int:
    import gi

    gi.require_version("Gtk", "3.0")
    gi.require_version("WebKit2", "4.1")
    from gi.repository import GLib, Gtk, WebKit2

    width, height = args.width, args.height
    win = Gtk.Window(title="shot-webkit")
    win.set_default_size(width, height)
    content = WebKit2.UserContentManager()
    if args.dark or args.light:
        scheme = "dark" if args.dark else "light"
        content.add_script(
            WebKit2.UserScript(
                SCHEME_SCRIPT.replace("SCHEME", scheme),
                WebKit2.UserContentInjectedFrames.ALL_FRAMES,
                WebKit2.UserScriptInjectionTime.START,
                None,
                None,
            )
        )
    view = WebKit2.WebView.new_with_user_content_manager(content)
    view.set_size_request(width, height)
    win.add(view)
    win.show_all()
    code = {"value": 0}
    state = {"started": False}

    def finish(value=0):
        code["value"] = value
        Gtk.main_quit()

    def run_js(script, then):
        def done(source, result, _data=None):
            try:
                value = source.evaluate_javascript_finish(result)
                then(json.loads(value.to_string()) if value is not None and value.is_string() else None)
            except GLib.Error as err:
                print(f"js error: {err.message}", file=sys.stderr)
                finish(1)

        # The result always goes through JSON.stringify: a plain JS value is easy to read from a string.
        wrapped = f"JSON.stringify((() => {{ return ({script}); }})() ?? null)"
        view.evaluate_javascript(wrapped, -1, None, None, None, done, None)

    def snapshot():
        def done(source, result, _data=None):
            surface = source.get_snapshot_finish(result)
            surface.write_to_png(args.out)
            finish()

        if args.out:
            view.get_snapshot(
                WebKit2.SnapshotRegion.VISIBLE, WebKit2.SnapshotOptions.NONE, None, done, None
            )
        else:
            finish()

    def after_print(_value=None):
        GLib.timeout_add(100, snapshot)

    def measure():
        if args.print_js:
            def printed(value):
                print(json.dumps(value))
                after_print()

            run_js(args.print_js, printed)
        else:
            after_print()

    def after_eval(_value=None):
        GLib.timeout_add(300 if args.eval_js else 0, measure)

    def ready(value):
        if value is not True:
            return False

        def go():
            if args.eval_js:
                run_js(args.eval_js, after_eval)
            else:
                after_eval()
            return False

        GLib.timeout_add(args.wait, go)
        return True

    def poll():
        if state["started"]:
            return False
        run_js(READY, lambda value: poll_done(value))
        return True

    def poll_done(value):
        if value is True and not state["started"]:
            state["started"] = True
            ready(value)

    def on_load(_view, event):
        if event == WebKit2.LoadEvent.FINISHED:
            GLib.timeout_add(100, poll)

    def on_failed(_view, _event, uri, error):
        print(f"error: loading {uri}: {error.message}", file=sys.stderr)
        finish(1)
        return True

    def on_timeout():
        print("error: the page was not ready in 60 s", file=sys.stderr)
        finish(1)

    view.connect("load-changed", on_load)
    view.connect("load-failed", on_failed)
    GLib.timeout_add_seconds(60, on_timeout)
    view.load_uri(quote(args.url, safe=":/?&=#%@+,;~!$'()*"))
    Gtk.main()
    return code["value"]


if __name__ == "__main__":
    sys.exit(main())
