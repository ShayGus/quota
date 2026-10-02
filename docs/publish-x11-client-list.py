#!/usr/bin/env python3
"""Publish _NET_CLIENT_LIST_STACKING on the X11 root window.

`tauri-plugin-mcp` takes screenshots with `xcap`, which finds top-level windows
only through that EWMH atom. WSLg's Weston helper never sets it, so
`take_screenshot` reports `Window not found`. Publish the app's window ids here
and it works.

Usage:

    xwininfo -root -tree            # find the window id, e.g. 0x600004
    docs/publish-x11-client-list.py 0x600004

Only the ids you pass are published. Both Quota windows can be published
together: application-name matching is disabled, so the requested label selects
the distinct title (`Quota` or `Quota settings`). Show the requested window
before capturing it. Needs only `libX11.so.6`, no Python packages.
"""

import ctypes
import sys

X11 = ctypes.CDLL("libX11.so.6")
X11.XOpenDisplay.restype = ctypes.c_void_p
X11.XOpenDisplay.argtypes = [ctypes.c_char_p]
X11.XDefaultRootWindow.restype = ctypes.c_ulong
X11.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
X11.XInternAtom.restype = ctypes.c_ulong
X11.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
X11.XChangeProperty.argtypes = [
    ctypes.c_void_p,
    ctypes.c_ulong,
    ctypes.c_ulong,
    ctypes.c_ulong,
    ctypes.c_int,
    ctypes.c_int,
    ctypes.c_void_p,
    ctypes.c_int,
]
X11.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]


def main() -> int:
    """Sets the stacking client list to the window ids given on the command line."""
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    display = X11.XOpenDisplay(None)
    if not display:
        print("no X display")
        return 1
    root = X11.XDefaultRootWindow(display)
    windows = (ctypes.c_ulong * (len(sys.argv) - 1))(
        *[int(arg, 16) for arg in sys.argv[1:]]
    )
    cardinality = X11.XInternAtom(display, b"CARDINAL", 0)
    atom = X11.XInternAtom(display, b"_NET_CLIENT_LIST_STACKING", 0)
    X11.XChangeProperty(
        display,
        root,
        atom,
        cardinality,
        32,
        0,
        ctypes.cast(windows, ctypes.c_void_p),
        len(windows),
    )
    X11.XSync(display, 0)
    print("published", " ".join(hex(window) for window in windows))
    return 0


raise SystemExit(main())
