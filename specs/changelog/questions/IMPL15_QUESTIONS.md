# impl15 — open questions / mismatches found by drawdiff

Answered in [answers_impl15](../answers_impl15.md).

Numbers: `cargo run --release --bin drawdiff -- specs/data/drawlog/<ref> <port log>` (port logs via `drawrun`).

1. **Boot timing**: the original presents ~4 empty frames (only a `C tz` on present 2) before the first logo frame; the port draws the logo from present 1, so the nowloading VMs are one frame ahead until the title. Is there a fixed delay before Loading_Begin is drawn (loader thread)?
2. **Loading VMs draw function**: inferred from the log (ring is rotated → Draw2DRot, mode 2) and positions relative to (500, 440) including pos_offset. Please confirm the Supervisor draw (0x445D3E).
3. **Practice stage select (12 §2.5)**: the original draws the list with ascii.anm#0/#1 glyphs ("Stage / HI-Score" header, rows at x 255.5…, y 223.5 + 16·n, ascii.anm#1 for the cursor row?) and hides the other teams' portraits; the port uses a stand-in ASCII list and shows faded portraits. Need the layout/colours and which VMs stay visible.
4. **Replay screen** (scene 1, screen 7): reference draws 36+ glyph quads the port lacks (header/list layout). Needs spec.
5. **Result viewer (scene 5)** and **music room (scene 8)** are stubs; the reference also leaves scene 5 after 200 frames via the X presses (port stub needs Shot/Enter). After music room the original captures the screen into surface 8 (`K screen th08logo.jpg`) and shows it during the menu loading.
6. **Demo frame 1 black overlay**: the reference draws a full-screen `Q … 80000000` (alpha 0x80 black) on the first gameplay frames of every demo (9 frames); the port draws nothing. Which effect is that (type 5 darken with release)?
7. **Gauge readout start**: on demo frame 1 the original's readout is still sliding (max-value line at y 458.5 vs 453.5 in the port) and the zone markers (alpha 0x85 in the port) are not drawn yet. Timing of interrupts 1/2/3 at stage start?
8. **HUD z / zwrite**: the original's front.anm panel quads carry z 0.46/0.49 and zwrite 1, ascii text zwrite 1; the port uses 0 (invisible with ztest=always; reported as "2D z/zwrite").
9. **Stage particles** (etama.anm#1, additive, `ztest=le`, demos 1 and 3): positions/colours differ from frame 1 while the demo trace is still in sync — probably RNG source or spawn order of the stage sparkle effects (tag 0x1X billboards / type 0x3E). Please check which RNG and order they use.
10. **Panel tiles over the playfield edge**: with `--playfield` the port shows 16 extra `front.anm#0` quads per frame (frame tiles redrawn every frame, the original only on dirty rows). Expected with the persistent back buffer; noted so they are not chased.
11. **stgNtxt.anm state**: stage title quads differ only in state (blend/zwrite) — the S lines need a look (`stg5txt.anm#0` state 1496 in demo 0).
12. Difficulty screen: VM 131+d slide timing after cancelling from character select now matches, but the general rule "screen enter → cursor interrupts on the same frame" was inferred from the log.
