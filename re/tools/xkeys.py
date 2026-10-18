#!/usr/bin/env python3
"""xkeys.py <display> <script> : send real X key events through XTest (test driver for record_session.sh on Xvfb).
Script lines: `wait S` | `press KEY [HOLD_S]` | `down KEY` | `up KEY` | `# comment`. KEY = z x up down left right
esc shift ctrl enter or an X keysym name."""
import ctypes, sys, time

KEYS = {"up": "Up", "down": "Down", "left": "Left", "right": "Right", "esc": "Escape", "shift": "Shift_L",
        "ctrl": "Control_L", "enter": "Return"}


def main(disp, script):
    x11 = ctypes.CDLL("libX11.so.6")
    xt = ctypes.CDLL("libXtst.so.6")
    x11.XOpenDisplay.restype = ctypes.c_void_p
    x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x11.XStringToKeysym.restype = ctypes.c_ulong
    x11.XStringToKeysym.argtypes = [ctypes.c_char_p]
    x11.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
    x11.XFlush.argtypes = [ctypes.c_void_p]
    xt.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
    xt.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_ulong]
    d = x11.XOpenDisplay(disp.encode())
    if not d:
        sys.exit("cannot open display " + disp)

    def code(k):
        ks = x11.XStringToKeysym(KEYS.get(k, k).encode())
        return x11.XKeysymToKeycode(d, ks)

    def ev(k, down):
        xt.XTestFakeKeyEvent(d, code(k), 1 if down else 0, 0)
        x11.XFlush(d)

    xt.XTestFakeMotionEvent(d, -1, 300, 200, 0)
    x11.XFlush(d)
    for line in open(script):
        t = line.split("#")[0].split()
        if not t:
            continue
        if t[0] == "wait":
            time.sleep(float(t[1]))
        elif t[0] == "press":
            ev(t[1], True)
            time.sleep(float(t[2]) if len(t) > 2 else 0.08)
            ev(t[1], False)
            time.sleep(0.02)
        elif t[0] == "down":
            ev(t[1], True)
        elif t[0] == "up":
            ev(t[1], False)
        print(time.strftime("%H:%M:%S"), " ".join(t), flush=True)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
