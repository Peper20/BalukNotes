#!/usr/bin/env python3
"""A page screenshot or a measurement in WebKitGTK (the engine of the desktop window).

Counterpart of tools/shot.mjs for WebKit: opens the URL in a real (visible,
briefly shown on the desktop) Gtk.Window - a hidden OffscreenWindow without GL
crashes WebKit - waits for <html data-state="ready"> and document.fonts.ready,
then saves a PNG and/or prints the value of a JS expression.

  tools/shot-webkit.py <url> [out.png] [--size 1300x900] [--dark|--light] [--wait 300]
                       [--eval 'JS'] [--print 'JS expression']

  --dark / --light   what matchMedia("(prefers-color-scheme: ...)") answers (without them - the system one)
  --wait   extra time after rendering, ms
  --eval   run before the screenshot / measurement (e.g. open the settings)
  --print  print the JSON value of a synchronous JS expression (a promise is not awaited)

Needs python `gi` with WebKit2 4.1 and Gtk 3 (webkit2gtk-4.1) and a graphical
session. Exit code 1 on a timeout.
"""

import argparse
import json
import sys
from urllib.parse import quote

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import GLib, Gtk, WebKit2  # noqa: E402

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


def main() -> int:
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
    width, height = (int(x) for x in args.size.split("x"))


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

    view.connect("load-changed", on_load)
    GLib.timeout_add_seconds(60, lambda: finish(1))
    view.load_uri(quote(args.url, safe=":/?&=#%@+,;~!$'()*"))
    Gtk.main()
    return code["value"]


if __name__ == "__main__":
    sys.exit(main())
