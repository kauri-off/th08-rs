# Port UI rules (Slint: `ui/*.slint`)

Sizes are mock-up points: 1 pt = 1 px at 1920×1080 (`portmenu::layout`). On a touch screen 120 pt ≥ 48 dp.

**One component set.** Screens compose `ui/components.slint`; no screen draws its own control. Colours and faces only
from `Tk` (`ui/tokens.slint`). Corners 0 (keycap 3). Text is a `Label` / `Ringed`, targets are a `Target`.

**Grid.** A page has one label column and one value column. Label column = the widest unbreakable label piece + 24,
at least 36 % of the content (42 % under 720 pt, at most 440 on wide panes), at most 50 %. Values start at the value
column; sliders fill it.

**Row heights.** Wide 70, split phone 132; a row grows with wrapped content, in steps of its own content height.

**Choices.** Words sit side by side while they fit the value column. When they do not, the row is a ◀ value ▶ stepper;
choices never wrap. Selected word white, others dim, no box.

**Narrow panes.** A row stacks its control under the label when a label piece is wider than the label column, or
when the stepper with its widest word does not fit. A word is never broken inside; Japanese breaks only between BudouX
phrases, with kinsoku (`slintui/text.rs`).

**Targets.** Every tap target on touch is at least 48 dp tall; steppers, choices and tabs also 48 dp wide. Exception:
the list's paging margin.

**Bars.** Tabs shrink to the least font (13 dp), then share the bar equally and wrap. Hints and the device line take two
lines when one does not fit. A slider that does not fit the value column goes under its label.

**Lists scroll.** A list longer than its pane is a `ScrollList`: key focus reveals its row; ▴ ▾ marks in the right
margin show more content and page the list. The rule above the danger row is part of that row.

**Focus.** Exactly one focus bar, only after key or pad input. Pointer hover lights a row while it is there. Touch
shows a pressed band, no bar. The first key after pointer or touch shows the bar and moves nothing.

**Help line** follows the focused row (keys, touch: the last touched row; mouse: the hovered row). **Hint line:** the
active device only: keycaps on keyboard, discs on pad, none on touch.

**Danger actions** are text only, end in "…" and ask in place (`Confirm`); a second press confirms, anything else
cancels.

| Input | Does |
|---|---|
| ↑ ↓ / d-pad | move the row (up from the first row: tab bar) |
| ← → | change the value (on the tab bar: tab) |
| Z, Enter, Space / A | activate |
| X, Backspace / B | back |
| Q E, PgUp PgDn / LB RB | previous, next tab, wrapping |
| Wheel | scroll the list under the pointer |
| Tap | activate what is under the finger |
| Drag | scroll a list and change nothing; sideways on a slider track (past 8 pt) sets it |

Checks: `uishots --only=slint` reads the layout from element queries (`slintui/check.rs`), and
`crates/th08-tools/tests/slint_input.rs` injects each input.
