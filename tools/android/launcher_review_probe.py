#!/usr/bin/env python3
"""Checks the launcher review regressions on one Japanese font-scale 1.5 emulator."""
import argparse
import json
import os
from pathlib import Path

from ui_matrix import (
    Emulator, PKG, Ui, configure, ensure_rotation, go, install_fx, launch, open_page, stats_tab, strings,
)


def require(condition, detail):
    if not condition:
        raise RuntimeError(detail)
    print(detail, flush=True)


def tap(ui, *labels):
    if not ui.tap_label(*labels):
        raise RuntimeError(f"Could not tap {labels!r}. Visible labels: {ui.dump().texts()}")


def rendered(ui, name):
    ui.shot(name)
    reply = json.loads(ui.emu.sh(
        f"run-as {PKG} rm -f cache/text_audit.json && "
        f"am broadcast -n {PKG}/org.th08.port.TextAuditReceiver --ez geometry true >/dev/null && "
        f"run-as {PKG} cat cache/text_audit.json", timeout=30))
    (ui.out / f"{name}-layout.json").write_text(json.dumps(reply, ensure_ascii=False, indent=2))
    return reply["layouts"]


def home(ui, st):
    title = st["import_files"]
    label = next(item for item in rendered(ui, "home_import") if item["text"] == title)
    drawn = "".join(line["text"] for line in label["lines"])
    require(not label["clipped"] and drawn == title, f"Home import text layout: {label}")


def spell_cards(ui, st):
    go(ui, st, "stats_title")
    ui.tap(stats_tab(ui, st["stats_tab_cards"]))
    layouts = rendered(ui, "spell_cards")
    name = "灯符「ファイヤフライフェノメノン」"
    spell = next(item for item in layouts if item["text"].replace("\n", "") == name)
    require(not spell["clipped"] and any("フェノメノン" in line["text"] for line in spell["lines"]),
            f"Japanese spell title layout: {spell['lines']}")
    for heading, count in [("stats_mode_story", "1 / 1"), ("stats_mode_spell", "0 / 0")]:
        h, c = (next(item for item in layouts if item["text"] == text) for text in (st[heading], count))
        right_h, right_c = h["lines"][-1]["right"], c["lines"][-1]["right"]
        require(not h["clipped"] and not c["clipped"] and abs(right_h - right_c) < 2,
                f"{st[heading]} and {count} rendered right edges: {right_h:.1f}px and {right_c:.1f}px")


def editor(ui, st, combo):
    go(ui, st, "nav_settings")
    require(open_page(ui, st, st["section_sound"]), "Opened the Sound settings category")
    row = ui.scroll_to(match=lambda n: n.text == "全体音量")
    ui.emu.sh(f"input tap {row.cx} {row.cy}")
    field = next(n for n in ui.dump().nodes if n.cls.endswith("EditText"))
    ui.tap(field)
    ui.emu.sh("input keyevent KEYCODE_MOVE_END KEYCODE_DEL KEYCODE_DEL KEYCODE_DEL && input text 73")
    require(any(n.cls.endswith("EditText") and n.text == "73" for n in ui.dump().nodes), "Draft is 73 before rotation")
    require(ui.value("volume_master") is None, "The draft has not reached port.cfg")
    for land in (True, False):
        ui.emu.sh(f"wm user-rotation lock {int(land)}")
        ensure_rotation(ui, ui.emu, combo | {"land": land})
        s = ui.dump()
        require(any(n.cls.endswith("EditText") and n.text == "73" for n in s.nodes),
                f"Draft is 73 in {'landscape' if land else 'portrait'}")
    ui.shot("draft_after_rotation")
    tap(ui, st["ok"])
    require(ui.value("volume_master") == "73", "Confirmed draft saved as volume_master = 73")


def replay(ui, st):
    go(ui, st, "replays_title")
    s = ui.dump()
    row = s.find("th8_01.rpy")
    if row is None:
        raise RuntimeError("Replay fixture th8_01.rpy is absent")
    card = row
    while card.parent is not None:
        more = [n for n in s.find_all(st["more"]) if card.x0 <= n.x0 and n.x1 <= card.x1
                and card.y0 <= n.y0 and n.y1 <= card.y1 and n.click_target()]
        if more:
            ui.tap(more[0])
            break
        card = card.parent
    else:
        raise RuntimeError("Replay card overflow action is absent")
    tap(ui, st["last_failed_details"])
    s = ui.dump()
    expected = ["th8_01.rpy", st["replays_player"].replace("%1$s", "PN"),
                st["replays_slowdown"].replace("%1$.2f%%", "0.00%")]
    require(all(s.find(text) for text in expected), f"Replay Details fields: {s.texts()}")
    ui.shot("replay_details", s)
    tap(ui, st["ok"])


def import_feedback(ui, st):
    rejected = st["import_wrong_file"].replace("%s", "")
    go(ui, st, "nav_settings")
    require(open_page(ui, st, st["section_app"]), "Opened the Game and app category")
    fixture = ui.out / "not-game.txt"
    fixture.write_text("wrong input for the game import")
    ui.emu.adb("push", fixture, "/sdcard/Download/th08-review-bad.txt")
    ui.emu.sh("am broadcast -a android.intent.action.MEDIA_SCANNER_SCAN_FILE "
              "-d file:///sdcard/Download/th08-review-bad.txt", check=False)
    ui.tap(ui.scroll_to(st["import_again"]))
    ui.foreign_ok = True
    try:
        picker = ui.dump()
        if picker.find("th08-review-bad.txt", clickable=True) is None:
            tap(ui, "Show roots", "Open navigation drawer")
            tap(ui, "Downloads")
        tap(ui, "th08-review-bad.txt")
    finally:
        ui.foreign_ok = False
    require(ui.scroll_to(match=lambda n: n.text.startswith(rejected)) is not None, "Settings shows the failed import")
    go(ui, st, "nav_home")
    link = st["game_data"] + ": " + st["data_imported"]
    require(ui.dump().find(link, clickable=True), f"Home retains its ready-state link after import feedback: {link}")
    tap(ui, link)
    ui.page = st["section_app"]
    ui.tap(ui.scroll_to(st["import_remove"]))
    tap(ui, st["import_remove"])
    ui.wait(.5)
    require(ui.scroll_to(st["import_files"]) is not None, "Settings shows the missing-data import action after removal")
    require(not any(n.text.startswith(rejected) for n in ui.dump().nodes), "Settings discarded the old import result")
    go(ui, st, "nav_home")
    s = ui.dump()
    require(s.find(st["import_files"]) is not None and not any(n.text.startswith(rejected) for n in s.nodes),
            "Home shows missing data without the old import result")
    ui.shot("home_after_removal", s)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--apk", required=True)
    p.add_argument("--out", required=True, type=Path)
    args = p.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    emu = Emulator(int(os.environ["QEMU_PORT"]))
    combo = dict(land=False, lang="ja", font="1.5", dark=True, cutout=True, imported=False)
    st = strings(combo["lang"])
    try:
        emu.boot(args.out / "emulator.log")
        emu.adb("install", "-r", "-d", args.apk, timeout=180)
        configure(emu, combo)
        install_fx(emu, args.out, combo, plain=True)
        launch(emu)
        ui = Ui(emu, args.out, combo)
        ensure_rotation(ui, emu, combo)
        home(ui, st)
        combo.update(imported=True, cutout=False, dark=False)
        configure(emu, combo)
        install_fx(emu, args.out, combo, plain=True)
        launch(emu)
        spell_cards(ui, st)
        editor(ui, st, combo)
        replay(ui, st)
        import_feedback(ui, st)
    finally:
        emu.kill()


if __name__ == "__main__":
    main()
