#!/usr/bin/env -S python3 -I
"""Drives the Android launcher on an emulator with real input across the UI matrix and writes contact sheets."""
import argparse, itertools, json, os, re, shutil, subprocess, sys, tempfile, time, xml.etree.ElementTree as ET
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SDK = Path(os.environ.get("ANDROID_SDK") or os.environ.get("ANDROID_HOME") or Path.home() / ".local/share/android-sdk/Sdk")
ADB = str(SDK / "platform-tools/adb")
PKG = "org.th08.port.debug"
ACTIVITY = f"{PKG}/org.th08.port.LauncherActivity"
CUTOUT = "com.android.internal.display.cutout.emulation.tall"
LANGS = ["en", "ru", "ja"]
DP_MIN = 48
FULL = {(True, "en", "1.0", False), (True, "ru", "1.5", True), (False, "ja", "1.5", True)}


def run(*a, check=True, timeout=120, stdin=None):
    r = subprocess.run(a, capture_output=True, timeout=timeout, input=stdin)
    if check and r.returncode:
        raise RuntimeError(f"{' '.join(map(str, a))}: {r.stderr.decode(errors='replace')[:400]}")
    return r.stdout


class Emulator:
    def __init__(self, port):
        self.serial, self.proc = f"emulator-{port}", None
        self.port = port

    def adb(self, *a, **k):
        return run(ADB, "-s", self.serial, *map(str, a), **k)

    def sh(self, cmd, **k):
        return self.adb("shell", cmd, **k).decode(errors="replace").strip()

    def boot(self, log):
        env = {k: v for k, v in os.environ.items() if k != "WAYLAND_DISPLAY"}
        self.proc = subprocess.Popen(
            [str(SDK / "emulator/emulator"), "-avd", "th08dbg", "-port", str(self.port), "-no-window", "-no-audio",
             "-no-boot-anim", "-read-only", "-no-snapshot", "-gpu", "swiftshader_indirect"],
            stdout=open(log, "wb"), stderr=subprocess.STDOUT, env=env)
        self.adb("wait-for-device", timeout=300)
        end = time.time() + 400
        while self.sh("getprop sys.boot_completed", check=False) != "1":
            if time.time() > end:
                raise RuntimeError("emulator did not boot")
            time.sleep(2)
        self.sh("settings put global window_animation_scale 0; settings put global transition_animation_scale 0; "
                "settings put global animator_duration_scale 0; settings put system accelerometer_rotation 0; "
                "settings put secure show_ime_with_hard_keyboard 0; input keyevent 82")

    def kill(self):
        if self.proc:
            self.adb("emu", "kill", check=False)
            try:
                self.proc.wait(60)
            except subprocess.TimeoutExpired:
                self.proc.kill()


class Node:
    def __init__(self, e, parent):
        self.a, self.parent = e.attrib, parent
        m = re.findall(r"\d+", self.a.get("bounds", "[0,0][0,0]"))
        self.x0, self.y0, self.x1, self.y1 = map(int, m)
        self.text = self.a.get("text", "")
        self.desc = self.a.get("content-desc", "")
        self.label = self.text or self.desc
        self.clickable = self.a.get("clickable") == "true"
        self.checkable = self.a.get("checkable") == "true"
        self.checked = self.a.get("checked") == "true"
        self.scrollable = self.a.get("scrollable") == "true"
        self.cls = self.a.get("class", "")

    w = property(lambda s: s.x1 - s.x0)
    h = property(lambda s: s.y1 - s.y0)
    cx = property(lambda s: (s.x0 + s.x1) // 2)
    cy = property(lambda s: (s.y0 + s.y1) // 2)

    def click_target(self):
        n = self
        while n is not None:
            if n.clickable:
                return n
            n = n.parent
        return None


class Screen:
    def __init__(self, xml_text):
        self.nodes = []

        def walk(e, parent):
            n = Node(e, parent) if e.tag == "node" else None
            if n is not None:
                self.nodes.append(n)
            for c in e:
                walk(c, n if n is not None else parent)
            if n is not None and not n.label:
                n.label = next((x.label for x in self.nodes if x.parent is n and x.label), "")
        walk(ET.fromstring(xml_text), None)

    def find_all(self, *labels):
        return [n for n in self.nodes for l in labels if l and (n.text == l or n.desc == l or n.text.startswith(l + "\n"))]

    def find(self, *labels, clickable=False):
        hits = self.find_all(*labels)
        tappable = [n for n in hits if n.click_target()]
        return (tappable or ([] if clickable else hits) or [None])[0]

    def below(self, row, *labels):
        """The tappable match nearest under the row title, in its column: its segment, not a same-named value elsewhere."""
        grids = [n for n in self.nodes if n.scrollable and n.x0 <= row.x0 and n.x1 >= row.x1]
        mid = (grids[0].x0 + grids[0].x1) // 2 if grids else None
        same = lambda n: mid is None or (n.cx < mid) == (row.cx < mid)
        hits = sorted((n for n in self.find_all(*labels) if n.click_target() and n.y0 >= row.y0 and same(n)), key=lambda n: n.y0)
        return hits[0] if hits else None

    def texts(self):
        return [n.label for n in self.nodes if n.label]

    def toggles(self):
        return sorted((n.y0 > -1 and n.label, n.checked) for n in self.nodes if n.checkable)


class Ui:
    def __init__(self, emu, out, combo):
        self.emu, self.out, self.combo = emu, out, combo
        self.problems, self.shots, self.n = [], [], 0
        self.dpi = int(re.search(r"\d+$", emu.sh("wm density").splitlines()[-1]).group())
        self.seen_problem = set()

    @property
    def dp(self):
        return self.dpi / 160

    def size(self):
        m = re.search(r"cur=(\d+)x(\d+)", self.emu.sh("dumpsys window displays", timeout=30))
        return int(m[1]), int(m[2])

    def dump(self):
        for _ in range(6):
            out = self.emu.adb("exec-out", "uiautomator dump /dev/tty", check=False, timeout=60).decode(errors="replace")
            i = out.find("<hierarchy")
            j = out.rfind("</hierarchy>")
            if i >= 0 and j > 0:
                return Screen(out[i:j + 12])
            time.sleep(1)
        raise RuntimeError("uiautomator dump failed")

    def problem(self, kind, detail):
        key = (kind, detail)
        if key not in self.seen_problem:
            self.seen_problem.add(key)
            self.problems.append({"combo": self.combo, "kind": kind, "detail": detail, "shot": self.n})

    def lint(self, s, where):
        W, H = self.size()
        cut = self.cutout_rect()
        window = [n for n in s.nodes if n.parent is None]
        leaves = [n for n in s.nodes if n.text]
        vp = self.scroll_region(s)
        st = strings(self.combo["lang"])
        covers = [c for c in (s.find(st["play"]), s.find(st["play_need_data"])) if c is not None]
        for n in s.nodes:
            full = n.y0 > 0 and n.y1 < H - 1 and not (vp and (n.y0 <= vp[1] + 2 or n.y1 >= vp[3] - 2))
            pad = 8 * self.dp
            covered = any(c is not n and n.x1 > c.x0 - pad and n.x0 < c.x1 + pad and n.y1 > c.y0 - pad and n.y0 < c.y1 + pad for c in covers)
            if n.clickable and n.w and n.h and full and not covered and (n.w < DP_MIN * self.dp - 2 or n.h < DP_MIN * self.dp - 2):
                self.problem("small target", f"{where}: '{n.label or n.cls}' {round(n.w / self.dp)}x{round(n.h / self.dp)} dp")
            if (n.text or n.desc) and cut and n.x0 < cut[2] and n.x1 > cut[0] and n.y0 < cut[3] and n.y1 > cut[1] and n.w < W * .98:
                self.problem("cut-out overlap", f"{where}: '{n.label[:40]}' x {n.x0}..{n.x1} y {n.y0}..{n.y1}, cut-out {cut}")
            if n.x1 > W + 1 or n.x0 < -1:
                self.problem("off-screen", f"{where}: '{n.label[:40]}' x {n.x0}..{n.x1}")
        for n in leaves:
            if len(n.text) >= 6 and n.w < 3.2 * 14 * self.dp and n.h > 2.2 * 18 * self.dp:
                self.problem("squeezed text", f"{where}: '{n.text[:40]}' {round(n.w / self.dp)} dp wide")
        for a, b in itertools.combinations(leaves, 2):
            ox = min(a.x1, b.x1) - max(a.x0, b.x0)
            oy = min(a.y1, b.y1) - max(a.y0, b.y0)
            if ox > 4 * self.dp and oy > 6 * self.dp and a.parent is b.parent and a.text != b.text:
                self.problem("overlapping text", f"{where}: '{a.text[:30]}' / '{b.text[:30]}'")

    def cutout_rect(self):
        if not self.combo.get("cutout"):
            return None
        W, H = self.size()
        m = re.search(r"mDisplayCutout=.*?safeInsets=Rect\((\d+), (\d+) - (\d+), (\d+)\)|insets=Rect\((\d+), (\d+) - (\d+), (\d+)\)",
                      self.emu.sh("dumpsys window displays", timeout=30))
        ins = [int(v) for v in m.groups() if v is not None] if m else [0, 0, 0, 0]
        l, t, r, b = ins
        if W > H and t and not (l or r):
            l, t = t, 0
        if l:
            return (0, 0, l, H)
        if r:
            return (W - r, 0, W, H)
        if t:
            return (0, 0, W, t)
        return None

    def shot(self, name, s=None):
        self.n += 1
        fn = self.out / f"{self.n:03d}_{name}.png"
        fn.write_bytes(self.emu.adb("exec-out", "screencap -p", timeout=60))
        self.shots.append(fn)
        self.lint(s or self.dump(), name)
        return fn

    def wait(self, t=1.0):
        time.sleep(t)

    def tap(self, n):
        t = n.click_target() or n
        self.emu.sh(f"input tap {t.cx} {t.cy}")
        self.wait(1.2)

    def tap_label(self, *labels):
        s = self.dump()
        n = s.find(*labels, clickable=True)
        if n is None:
            return False
        self.tap(n)
        return True

    def back(self):
        self.emu.sh("input keyevent 4")
        self.wait(1.0)

    def swipe_up(self, frac_x, region):
        x0, y0, x1, y1 = region
        x = int(x0 + (x1 - x0) * frac_x)
        h = y1 - y0
        self.emu.sh(f"input swipe {x} {int(y0 + h * .85)} {x} {int(y0 + h * .2)} 450")
        self.wait(.9)

    def swipe_down(self, frac_x, region):
        x0, y0, x1, y1 = region
        x = int(x0 + (x1 - x0) * frac_x)
        h = y1 - y0
        self.emu.sh(f"input swipe {x} {int(y0 + h * .2)} {x} {int(y0 + h * .85)} 300")
        self.wait(.6)

    def scroll_region(self, s):
        rs = [n for n in s.nodes if n.scrollable and n.w > 100 and n.h > 100]
        if not rs:
            return None
        rs.sort(key=lambda n: -(n.w * n.h))
        n = rs[0]
        return (n.x0, n.y0, n.x1, n.y1)

    def cfg(self):
        return self.emu.sh(f"run-as {PKG} cat files/port.cfg", check=False)

    def scroll_all(self, name, frac_x=.5, want=(), shots=True, max_swipes=60):
        """Swipes to the end by real gestures; returns every label seen and checks no value changed."""
        s = self.dump()
        region = self.scroll_region(s)
        seen, before_cfg = set(s.texts()), self.cfg()
        tog = {}
        for n in s.nodes:
            if n.checkable and n.w > 200 * self.dp:
                tog[n.label] = n.checked
        if shots:
            self.shot(f"{name}_top", s)
        if region is None:
            return seen, True
        prev, i = None, 0
        while i < max_swipes:
            i += 1
            self.swipe_up(frac_x, region)
            s = self.dump()
            seen |= set(s.texts())
            for n in s.nodes:
                if n.checkable and n.label and n.w > 200 * self.dp:
                    if n.label in tog and tog[n.label] != n.checked:
                        self.problem("scroll changed a toggle", f"{name}: {n.label}")
                    tog[n.label] = n.checked
            sig = tuple(s.texts())
            if sig == prev:
                break
            prev = sig
            if shots and i % 3 == 0:
                self.shot(f"{name}_s{i}", s)
        if shots:
            self.shot(f"{name}_end", s)
        if self.cfg() != before_cfg:
            self.problem("scroll changed a value", f"{name}: port.cfg differs")
        missing = [w for w in want if w not in seen and not any(t.startswith(w) for t in seen)]
        for w in missing:
            self.problem("unreachable", f"{name}: '{w}'")
        for _ in range(i + 2):
            self.swipe_down(frac_x, region)
        return seen, not missing

    def scroll_to(self, *labels, clickable=True, tries=40):
        s = self.dump()
        region = self.scroll_region(s)
        for down in (True, False):
            prev = None
            for _ in range(tries):
                n = s.find(*labels, clickable=clickable)
                room = 150 * self.dp if down else 4 * self.dp
                if n is not None and (region is None or (n.y0 >= region[1] and n.y1 <= region[3] - room)):
                    return n
                if region is None:
                    return None
                (self.swipe_up if down else self.swipe_down)(.25, region)
                s = self.dump()
                sig = tuple(s.texts())
                if sig == prev:
                    n = s.find(*labels, clickable=clickable)
                    if n is not None and n.y0 >= region[1] and n.y1 <= region[3]:
                        return n
                    break
                prev = sig
        return None


def strings(lang):
    p = REPO / "android/app/src/main/res" / ("values" if lang == "en" else f"values-{lang}") / "strings.xml"
    out = {}
    for e in ET.parse(p).getroot():
        if e.tag == "string":
            out[e.get("name")] = (e.text or "").strip('"').replace("\\'", "'").replace("\\n", "\n")
    return out


def schema(tmp):
    p = tmp / "schema.json"
    if not p.exists():
        subprocess.run(["cargo", "run", "-q", "--release", "-p", "th08-tools", "--bin", "settings_schema", str(p)], cwd=REPO, check=True)
    return json.loads(p.read_text())


def text_of(e, lang):
    return e[lang] if lang != "en" and lang in e else e


def fixtures(tmp, imported):
    d = tmp / ("fx_imported" if imported else "fx_missing")
    if d.exists():
        return d
    (d / "replay").mkdir(parents=True)
    src = REPO / "specs/data/sessions/gameover_save/replays/th8_02.rpy"
    other = REPO / "specs/data/sessions/hiscore_entry/replays/th8_01.rpy"
    shutil.copy(src, d / "replay/th8_02.rpy")
    shutil.copy(other, d / "replay/th8_01.rpy")
    shutil.copy(src, d / "replay/th8_a_really_long_replay_file_name_that_keeps_going_and_going_2026.rpy")
    (d / "replay/th8_damaged.rpy").write_bytes(b"not a replay" * 20)
    (d / "update_test.json").write_text(json.dumps({
        "tag_name": "v9.9.9", "html_url": "https://example.invalid/r", "draft": False, "prerelease": False,
        "assets": [{"name": "th08-9.9.9-android-apk.apk", "browser_download_url": "https://example.invalid/a.apk"},
                   {"name": "SHA256SUMS", "browser_download_url": "https://example.invalid/SHA256SUMS"}]}))
    (d / "last_error.txt").write_text("th08 0.1.0\n\npanic:\nfixture failure: could not open a very long path "
                                      "/data/user/0/org.th08.port.debug/files/game/th08.pak for reading\n\nbacktrace:\n  0: x\n")
    if imported:
        (d / "game").mkdir()
        (d / "game/th08.pak").write_bytes(b"x")
        (d / "game/thbgm.pak").write_bytes(b"x")
        shutil.copy(REPO / "specs/data/sessions/golden/end/score.dat", d / "score.dat")
        v = re.search(r"const VERSION: u\d+ = (\d+)", (REPO / "src/stats.rs").read_text())[1]
        lines, t = [], 1790000000
        for run_ in range(25):
            t += 1000
            ev = [{"ev": "start", "mode": "story", "shot": run_ % 12, "diff": run_ % 5, "stage": 0}]
            ev += [{"ev": "stage", "stage": s} for s in range(1, run_ % 6 + 1)]
            ev += [{"ev": "death", "stage": 1}, {"ev": "bomb", "stage": 2},
                   {"ev": "spell", "card": 3 + run_ % 4, "stage": 1, "name": "Fixture Sign \"An Unusually Long Spell Card Name\""},
                   {"ev": "spell_end", "card": 3 + run_ % 4, "captured": False},
                   {"ev": "end", "score": 123456780 * (run_ + 1), "stage": run_ % 6, "frames": 60000 + run_, "reason": "menu"}]
            lines += [json.dumps({**e, "v": int(v), "t": t}) for e in ev]
        (d / "stats.jsonl").write_text("\n".join(lines) + "\n")
    return d


def configure(emu, c):
    emu.sh(f"am force-stop {PKG}")
    emu.sh("wm user-rotation lock %d" % (1 if c["land"] else 0))
    emu.sh("settings put system font_scale %s" % c["font"])
    emu.sh("cmd uimode night %s" % ("yes" if c["dark"] else "no"))
    emu.sh(f"cmd overlay {'enable' if c['cutout'] else 'disable'} --user 0 {CUTOUT}", check=False)
    time.sleep(2.5)


def install_fx(emu, tmp, c):
    d = fixtures(tmp, c["imported"])
    tar = tmp / f"{d.name}.tar"
    if not tar.exists():
        subprocess.run(["tar", "-C", str(d), "-cf", str(tar), "."], check=True)
    emu.adb("push", tar, "/data/local/tmp/fx.tar")
    emu.sh(f"run-as {PKG} sh -c 'rm -rf files cache/*; mkdir files; cd files; tar xf /data/local/tmp/fx.tar; "
           f"printf \"language = {c['lang']}\\n\" > port.cfg'")


def launch(emu):
    emu.sh(f"am start -W -n {ACTIVITY}", timeout=90)
    time.sleep(3.5)


def ensure_rotation(ui, emu, c):
    """A rotation locked while the portrait-only home screen is on top can be dropped: re-lock with the app in front."""
    want = 1 if c["land"] else 0
    for _ in range(4):
        W, H = ui.size()
        if (W > H) == c["land"]:
            return
        emu.sh(f"wm user-rotation lock {1 - want}")
        time.sleep(1.5)
        emu.sh(f"wm user-rotation lock {want}")
        time.sleep(2.5)
    ui.problem("driver error", "could not set the orientation")


def walk_basic(ui, emu, c, sc):
    st, lang = strings(c["lang"]), c["lang"]
    ui.shot("home")
    ui.scroll_all("settings", .25, want=[text_of(e, lang)["label"] for e in sc["entries"] if "android" in e["platforms"] and e["audience"] != "dev"])
    n = ui.scroll_to(st["replays_browse"])
    if n:
        ui.tap(n)
        ui.shot("replays")
        ui.back()


def is_open(ui, st, labels=()):
    s = ui.dump()
    return s.find(st["cancel"], clickable=True) or s.find(st["ok"], clickable=True) or any(s.find(l) for l in labels)


def close_overlay(ui, st):
    for _ in range(3):
        if not is_open(ui, st):
            return
        ui.back()


def go(ui, st, tab):
    if not ui.tap_label(st[tab]):
        ui.problem("no destination", tab)


def check_play(ui, st, where):
    if ui.dump().find(st["play"]) is None:
        ui.problem("Play not on screen", where)


def exercise_row(ui, st, e, lang, n):
    label = text_of(e, lang)["label"]
    kind = e["kind"]["type"]
    words = {w["value"]: w["label"] for w in text_of(e, lang).get("words", [])}
    if n.click_target() is None:
        before, other = ui.cfg(), next((l for v, l in words.items() if v != e["default"]), None)
        s = ui.dump()
        seg = s.below(n, other) if other else None
        if seg is None:
            ui.problem("segment not found", label)
            return
        ui.tap(seg)
        if ui.cfg() == before:
            ui.problem("segment did not change the value", label)
        back = s.below(n, words.get(e["default"], ""))
        if back:
            ui.tap(back)
        return
    before = ui.cfg()
    ui.emu.sh(f"input tap {n.cx} {n.cy}")
    ui.wait(1.2)
    if kind == "bool":
        if ui.cfg() == before:
            ui.problem("switch did not change the value", label)
        ui.tap(n)
        if ui.cfg() != before:
            ui.problem("switch did not change back", label)
        return
    if ui.cfg() != before:
        ui.problem("opening a control changed its value", label)
    ui.shot(f"ctl_{e['key']}")
    if ui.dump().find(st["nav_settings"], clickable=True) is not None:
        ui.problem("control opened nothing", label)
        return
    ui.back()
    if ui.dump().find(st["cancel"], clickable=True):
        ui.back()


def slider_drag(ui, st, rows, lang):
    e = next(e for e in rows if e["key"] == "volume_master")
    n = ui.scroll_to(text_of(e, lang)["label"], clickable=False)
    s = ui.dump()
    bars = [b for b in s.nodes if "SeekBar" in b.cls and n is not None and b.y0 >= n.y0]
    if not bars:
        ui.problem("no slider node", "volume_master")
        return
    b, before = bars[0], ui.cfg()
    ui.emu.sh(f"input swipe {b.x0 + b.w * 9 // 10} {b.cy} {b.x0 + b.w // 2} {b.cy} 500")
    ui.wait()
    if ui.cfg() == before:
        ui.problem("slider drag did not change the value", "volume_master")
    ui.shot("slider_dragged")


def picker(ui, st):
    """Export through the system picker, then pick the zip for import; a miss is reported, not fatal."""
    n = ui.scroll_to(st["saves_export"])
    if n is None:
        ui.problem("unreachable", "export")
        return
    ui.tap(n)
    ui.wait(3)
    ui.shot("picker_export")
    s = ui.dump()
    save = s.find("SAVE", "Save", clickable=True)
    if save is None:
        ui.problem("not exercised", "export: no SAVE button in the picker")
        ui.back()
        return
    ui.tap(save)
    ui.wait(3)
    s = ui.dump()
    replace = s.find("OK", clickable=True)
    if replace:
        ui.tap(replace)
        ui.wait(3)
    ui.shot("export_done")
    n = ui.scroll_to(st["saves_import"])
    if n:
        ui.tap(n)
        ui.wait(3)
        ui.shot("picker_import")
        ui.back()


def walk(ui, emu, c, full_controls, sc):
    st, lang = strings(c["lang"]), c["lang"]
    rows = [e for e in sc["entries"] if "android" in e["platforms"]]
    player = [text_of(e, lang)["label"] for e in rows if e["audience"] != "dev"]
    ui.shot("home")
    check_play(ui, st, "home")
    if not c["imported"]:
        ui.tap_label(st["play"])
        s = ui.dump()
        if s.find(st["play_need_data"]) is None:
            ui.problem("no message for blocked Play", "missing data")
        ui.shot("play_blocked", s)
        ui.wait(5)
    elif full_controls:
        n = ui.dump().find(st["update_install"], clickable=True)
        if n:
            ui.tap(n)
            ui.wait(4)
            ui.shot("update_failed")
    go(ui, st, "nav_settings")
    check_play(ui, st, "settings")
    ui.scroll_all("settings_x25", .25, want=player)
    if full_controls:
        ui.scroll_all("settings_x85", .85, want=player, shots=False)
    n = ui.scroll_to(st["developer_options"])
    if n is None:
        ui.problem("unreachable", "developer options")
        ui.tap(n)
        ui.scroll_all("settings_dev", .5, want=[text_of(e, lang)["label"] for e in rows if e["audience"] == "dev"], shots=True)
        n = ui.scroll_to(st["developer_options"])
        if n:
            ui.tap(n)
    if full_controls:
        for e in rows:
            if e["audience"] == "dev":
                continue
            n = ui.scroll_to(text_of(e, lang)["label"], clickable=False)
            if n is None:
                ui.problem("unreachable", f"control '{e['key']}'")
                continue
            exercise_row(ui, st, e, lang, n)
        slider_drag(ui, st, rows, lang)
        s = ui.dump()
        info = s.find(st["about_setting"], clickable=True)
        if info:
            ui.tap(info)
            ui.shot("info")
            rotate(ui, emu, c, "info_dialog", st["ok"])
            close_overlay(ui, st)
        if ui.tap_label(st["more"]):
            ui.tap_label(st["reset_all"])
            ui.shot("reset_confirm")
            ui.back()
        picker(ui, st)
    go(ui, st, "replays_title")
    check_play(ui, st, "replays")
    ui.scroll_all("replays", .5, want=["th8_damaged.rpy"])
    if full_controls:
        d = ui.dump().find(st["replays_delete"], clickable=True)
        if d:
            ui.tap(d)
            ui.shot("replay_delete")
            rotate(ui, emu, c, "replay_delete", st["cancel"])
            close_overlay(ui, st)
        rotate(ui, emu, c, "replays", st["replays_sort_date"])
    go(ui, st, "stats_title")
    check_play(ui, st, "stats")
    ui.wait(1.5)
    ui.scroll_all("stats", .5, max_swipes=60 if full_controls else 6)


def rotate(ui, emu, c, name, keep):
    emu.sh("wm user-rotation lock %d" % (0 if c["land"] else 1))
    time.sleep(2.5)
    s = ui.dump()
    ui.shot(f"{name}_rotated", s)
    if s.find(keep) is None:
        ui.problem("state lost on rotation", f"{name}: '{keep}' gone")
    emu.sh("wm user-rotation lock %d" % (1 if c["land"] else 0))
    time.sleep(2.5)


def sheet(shots, out, title):
    from PIL import Image, ImageDraw
    if not shots:
        return
    thumbs = []
    for p in shots:
        im = Image.open(p).convert("RGB")
        im.thumbnail((360, 360))
        thumbs.append((p.stem, im))
    cols = min(6, len(thumbs))
    cw, ch = 372, 392
    rows = (len(thumbs) + cols - 1) // cols
    W = Image.new("RGB", (cols * cw + 12, rows * ch + 40), (32, 32, 36))
    d = ImageDraw.Draw(W)
    d.text((12, 12), title, fill=(240, 240, 240))
    for i, (name, im) in enumerate(thumbs):
        x, y = 12 + (i % cols) * cw, 40 + (i // cols) * ch
        W.paste(im, (x, y))
        d.text((x, y + im.height + 4), name, fill=(200, 200, 200))
    W.save(out)


def matrix(mode):
    if mode == "smoke":
        return [dict(land=True, lang="ru", dark=False, font="1.5", cutout=True, imported=True, full=True)]
    if mode == "audit":
        return [dict(land=l, lang=g, font=f, cutout=k, dark=d, imported=m, full=u) for l, g, f, k, d, m, u in [
            (True, "en", "1.0", False, False, True, True), (False, "en", "1.0", False, True, False, False),
            (True, "ru", "1.5", True, False, True, True), (False, "ru", "1.5", True, True, True, False),
            (True, "ja", "1.5", True, True, False, False), (False, "ja", "1.0", False, False, True, False)]]
    out = []
    for i, (land, lang, font, cutout) in enumerate(itertools.product([False, True], LANGS, ["1.0", "1.5"], [False, True])):
        out.append(dict(land=land, lang=lang, font=font, cutout=cutout, dark=bool(i % 2), imported=i % 3 != 2,
                        full=(land, lang, font, cutout) in FULL))
    return out


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--apk", default=str(REPO / "target/android/th08-debug.apk"))
    p.add_argument("--out", required=True)
    p.add_argument("--port", type=int, default=os.environ.get("QEMU_PORT"), required="QEMU_PORT" not in os.environ, help="emulator console port (default: $QEMU_PORT of the emu job)")
    p.add_argument("--matrix", choices=["smoke", "audit", "full"], default="full")
    p.add_argument("--basic", action="store_true", help="only the home screen and the settings scroll")
    p.add_argument("--only", help="substring filter on the combo name")
    p.add_argument("--no-boot", action="store_true", help="use a running emulator")
    a = p.parse_args()
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    tmp = Path(tempfile.mkdtemp(prefix="ui_matrix", dir=out))
    emu = Emulator(a.port)
    results, code = [], 0
    try:
        if not a.no_boot:
            emu.boot(out / "emulator.log")
        emu.adb("install", "-r", "-d", a.apk, timeout=300)
        sc = schema(tmp)
        for c in matrix(a.matrix):
            name = f"{'land' if c['land'] else 'port'}_{c['lang']}_f{c['font']}_{'cut' if c['cutout'] else 'nocut'}_{'dark' if c['dark'] else 'light'}_{'imported' if c['imported'] else 'missing'}"
            if a.only and a.only not in name:
                continue
            d = out / name
            d.mkdir(exist_ok=True)
            configure(emu, c)
            install_fx(emu, tmp, c)
            launch(emu)
            ui = Ui(emu, d, c)
            ensure_rotation(ui, emu, c)
            t = time.time()
            try:
                if a.basic:
                    walk_basic(ui, emu, c, sc)
                else:
                    walk(ui, emu, c, c["full"], sc)
            except Exception as e:
                ui.problem("driver error", repr(e))
            sheet(ui.shots, out / f"{name}.jpg", name)
            print(f"{name}: {len(ui.problems)} problems, {len(ui.shots)} shots, {time.time() - t:.0f}s", flush=True)
            for pr in ui.problems:
                print(f"  [{pr['kind']}] {pr['detail']}", flush=True)
            results += ui.problems
            try:
                logcat = emu.adb("logcat", "-d", "-b", "crash", check=False).decode(errors="replace")
            except subprocess.TimeoutExpired:
                logcat = ""
                results.append({"combo": c, "kind": "driver error", "detail": "adb logcat timed out"})
            if PKG in logcat:
                print("  CRASH\n" + logcat[-1500:], flush=True)
                results.append({"combo": c, "kind": "crash", "detail": name})
                emu.adb("logcat", "-c", "-b", "crash", check=False)
        (out / "problems.json").write_text(json.dumps(results, indent=1, ensure_ascii=False))
        code = 1 if results else 0
        print(f"total problems: {len(results)}")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
        if not a.no_boot:
            emu.kill()
    sys.exit(code)


if __name__ == "__main__":
    main()
