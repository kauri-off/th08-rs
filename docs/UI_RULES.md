# Port UI rules (Slint: `ui/*.slint`)

Sizes are mock-up points: 1 pt = 1 px at 1920×1080 (`portmenu::layout`). On a touch screen 120 pt ≥ 48 dp.

**One component set.** Screens compose `ui/components.slint`; no screen draws its own control. Colours and faces only
from `Tk` (`ui/tokens.slint`). Corners 0 (keycap 3). Text is a `Label` / `Ringed`, targets are a `Target`.

**Grid.** A page has one label column and one value column. Label column = the widest unbreakable label piece + 24,
at least 440 pt on wide panes (42 % of the content under 720 pt), at most 50 %. On a phone it is the longest whole label + 16, between 30 % and 36 % of the content, so sliders keep a long track; a label longer than that wraps between words. Values start at the value
column; sliders fill it.

**Row heights.** Wide 70, split phone 120 (48 dp), on any touch screen at least 48 dp; a row grows with wrapped content, in steps of its own content height.

**Choices.** Words sit side by side while they fit the value column. When they do not, the row is a ◀ value ▶ stepper;
choices never wrap. Selected word white, others dim, no box.

**Sliders.** Touch: ◀ track value ▶ is one 48 dp control, the value between the track and ▶; a track shorter than 100 pt moves the control under its label. Keys and pointer: ◀ track ▶ value. The phone's quick volume sets the track under ◀ value ▶.

**Type on a phone (points).** Row labels 28, words 30, tab rail 30 (ru 28), help 24, device line 19; each at least 8–11 dp.

**Narrow panes.** A row stacks its control under the label when a label piece is wider than the label column, or
when the stepper with its widest word does not fit. A word is never broken inside; Japanese breaks only between BudouX
phrases, with kinsoku (`slintui/text.rs`).

**Targets.** Every tap target on touch is at least 48 dp tall; steppers, choices and tabs also 48 dp wide. Exception:
the list's paging margin.

**Bars.** Tabs shrink to the least font (13 dp), then share the bar equally and wrap. Hints and the device line take two
lines when one does not fit. A slider that does not fit the value column goes under its label.

**Lists scroll.** A list longer than its pane is a `ScrollList`: key focus reveals its row; ▴ ▾ marks in the right
margin show more content and page the list. A row less than 70 % in view fades out whole and takes no taps, on every
list, so a half row never shows above the help line. The rule above the danger row is part of that row.

**Focus.** Exactly one focus bar, only after key or pad input. Pointer hover lights a row while it is there. Touch
shows a pressed band, no bar. The first key after pointer or touch shows the bar and moves nothing.

**Help line** follows the focused row (keys, touch: the last touched row; mouse: the hovered row). **Hint line:** the
active device only: keycaps on keyboard, discs on pad, none on touch.

**Danger actions** are text only, end in "…" and ask in place. A settings row: a second press confirms, anything else
cancels. The pause card (`ConfirmBlock`): question, note, then the safe answer, which holds the focus, and the action.

**Pause card.** Text ring only, no glow. Heading caption flush left; the team shows except on the phone. Retry and Quit
exist only when the game's own pause path can take them. 16:9: right pillar; a short pillar drops help and device, then
the hints, last the notes (rows and the question always stay). 4:3: on the playfield, ending above the player; the device line goes first, then hints, then notes. Phone: run items in the left bar,
quick settings in the right; on touch Resume keeps its band and tick.

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
`crates/th08-tools/tests/slint_input.rs` injects each input (settings rows, card items, confirms).
