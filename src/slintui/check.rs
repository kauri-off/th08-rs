//! Layout check and scripted input for the Slint pages, from element queries and the mock clock.
use super::menu::{Menu, View};
use super::text::plain;
use super::{TextProbe, host};
use crate::i18n::Lang;
use crate::platform::portmenu::{self, CardRig, Do, Nav, Shot};
use crate::settings::Settings;
use crate::settings_store::SettingsStore as Model;
use crate::uicheck::{Issue, Kind};
use i_slint_backend_testing::{ElementHandle, ElementQuery, ElementRoot};
use slint::ComponentHandle;
use std::collections::{BTreeSet, HashMap};
use std::time::Duration;
use th08_render::gfx::{Gfx, OverlayTarget, SafeArea};
use winit::dpi::PhysicalPosition;
use winit::event::{DeviceId, MouseScrollDelta, Touch, TouchPhase, WindowEvent};

#[derive(Clone, Copy, Debug)]
pub struct R {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl R {
    fn of(e: &ElementHandle) -> Self {
        let (p, s) = (e.absolute_position(), e.size());
        Self {
            x: p.x,
            y: p.y,
            w: s.width,
            h: s.height,
        }
    }
    fn right(self) -> f32 {
        self.x + self.w
    }
    fn bottom(self) -> f32 {
        self.y + self.h
    }
    fn cut(self, o: Self) -> Self {
        let (x, y) = (self.x.max(o.x), self.y.max(o.y));
        Self {
            x,
            y,
            w: (self.right().min(o.right()) - x).max(0.0),
            h: (self.bottom().min(o.bottom()) - y).max(0.0),
        }
    }
    fn inside(self, o: Self) -> bool {
        self.x >= o.x - 0.5
            && self.y >= o.y - 0.5
            && self.right() <= o.right() + 0.5
            && self.bottom() <= o.bottom() + 0.5
    }
    fn hits(self, o: Self) -> bool {
        self.y < o.bottom() && o.y < self.bottom() && self.x < o.right() && o.x < self.right()
    }
    fn egui(self) -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(self.x, self.y), egui::vec2(self.w, self.h))
    }
}

struct TextEl {
    text: String,
    wrap: bool,
    size: f32,
    serif: bool,
    natural: f32,
    height: f32,
    r: R,
}

type Widths = HashMap<(String, u32, bool), f32>;

fn issue(kind: Kind, what: String, r: R) -> Issue {
    Issue {
        kind,
        what,
        rect: r.egui(),
    }
}

// Each item once: a query returns an item per component it inherits through.
fn elements(root: &impl ElementRoot) -> Vec<ElementHandle> {
    let mut seen = BTreeSet::new();
    ElementQuery::from_root(root)
        .match_descendants()
        .find_all()
        .into_iter()
        .filter(|e| {
            let (p, s) = (e.absolute_position(), e.size());
            let key = (
                e.accessible_id().unwrap_or_default().to_string(),
                e.accessible_label().unwrap_or_default().to_string(),
                [p.x, p.y, s.width, s.height].map(f32::to_bits),
            );
            seen.insert(key)
        })
        .collect()
}

fn ids(root: &impl ElementRoot, prefix: &str) -> Vec<(String, R)> {
    elements(root)
        .into_iter()
        .filter_map(|e| {
            let id = e.accessible_id()?;
            id.starts_with(prefix).then(|| (id.to_string(), R::of(&e)))
        })
        .collect()
}

fn texts(root: &impl ElementRoot) -> Vec<TextEl> {
    elements(root)
        .into_iter()
        .filter_map(|e| {
            let id = e.accessible_id()?;
            let p: Vec<&str> = id.split('|').collect();
            if p.len() != 6 || p[0] != "t" {
                return None;
            }
            let text = e.accessible_label()?.to_string();
            if text.is_empty() {
                return None;
            }
            Some(TextEl {
                text,
                wrap: p[1] == "w",
                size: p[2].parse().ok()?,
                serif: p[3] == "s",
                natural: p[4].parse().ok()?,
                height: p[5].parse().ok()?,
                r: R::of(&e),
            })
        })
        .collect()
}

fn width_of(probe: &TextProbe, widths: &mut Widths, text: &str, size: f32, serif: bool) -> f32 {
    *widths
        .entry((text.to_string(), size.to_bits(), serif))
        .or_insert_with(|| {
            probe.set_text(text.into());
            probe.set_size(size);
            probe.set_serif(serif);
            probe.get_natural()
        })
}

// What the check needs to know about the frame.
struct Frame<'a> {
    scale: f32,
    density: f32,
    touch: bool,
    keys: bool,
    list: Option<R>,
    rows: &'a [R],
}

// Text fit and overlap, stray ids, focus marks and 48 dp targets of any Slint page.
fn common_issues(
    root: &(impl ElementRoot + ComponentHandle),
    probe: &mut TextProbe,
    widths: &mut Widths,
    f: &Frame<'_>,
) -> Vec<Issue> {
    let mut out = Vec::new();
    let size = root.window().size();
    let screen = R {
        x: 0.0,
        y: 0.0,
        w: size.width as f32 / f.scale,
        h: size.height as f32 / f.scale,
    };
    let list = f.list;
    let all = texts(root);
    let mut shown: Vec<(usize, R)> = Vec::new();
    for (i, t) in all.iter().enumerate() {
        // Row text scrolled out of the list is not drawn; membership comes from the row, not from where it lies.
        let (cx, cy) = (t.r.x + t.r.w / 2.0, t.r.y + t.r.h / 2.0);
        let scrolled = f
            .rows
            .iter()
            .any(|r| cx >= r.x && cx <= r.right() && cy >= r.y && cy <= r.bottom());
        let view = match (scrolled, list) {
            (true, Some(l)) => t.r.cut(l),
            _ => t.r,
        };
        if view.h < 1.0 || view.w < 1.0 {
            continue;
        }
        shown.push((i, view));
        let name = plain(&t.text);
        if !scrolled && !t.r.inside(screen) {
            out.push(issue(
                Kind::Clipped,
                format!("{name:?} leaves the window"),
                t.r,
            ));
        }
        if t.r.right() > screen.right() + 0.5 {
            out.push(issue(
                Kind::Clipped,
                format!("{name:?} past the right edge"),
                t.r,
            ));
        }
        if !t.wrap && t.natural > t.r.w + 1.0 {
            out.push(issue(
                Kind::Clipped,
                format!("{name:?} needs {} in {}", t.natural, t.r.w),
                t.r,
            ));
        }
        if t.wrap && t.height > t.r.h + 1.0 {
            out.push(issue(
                Kind::Clipped,
                format!("{name:?} needs height {} in {}", t.height, t.r.h),
                t.r,
            ));
        }
        if crate::uicheck::raw_id(&name).is_some() {
            out.push(issue(Kind::RawId, format!("{name:?}"), t.r));
        }
    }
    for (n, &(i, a)) in shown.iter().enumerate() {
        for &(j, b) in &shown[n + 1..] {
            let c = a.cut(b);
            if c.w > 2.0 && c.h > 2.0 {
                out.push(issue(
                    Kind::Overlap,
                    format!("{:?} / {:?}", plain(&all[i].text), plain(&all[j].text)),
                    c,
                ));
            }
        }
    }
    let focus = ids(root, "focus");
    if focus.len() > 1 {
        out.push(issue(
            Kind::TwoFocus,
            format!("{} focus marks", focus.len()),
            screen,
        ));
    }
    if f.keys && focus.is_empty() {
        out.push(issue(
            Kind::NoFocus,
            "no focus mark under keys".into(),
            screen,
        ));
    }
    if !f.keys && !focus.is_empty() {
        out.push(issue(
            Kind::TwoFocus,
            "focus mark without keys".into(),
            screen,
        ));
    }
    if f.touch {
        let dp = f.scale / f.density;
        for (id, r) in ids(root, "target|") {
            let n = id.trim_start_matches("target|");
            if n.starts_with("page-") {
                continue;
            }
            let wide = n.starts_with("step") || n.starts_with("choice") || n.starts_with("tab");
            if r.h * dp < 47.5 || (wide && r.w * dp < 47.5) {
                out.push(issue(
                    Kind::FewRows,
                    format!("target {n:?} {:.0}x{:.0} dp", r.w * dp, r.h * dp),
                    r,
                ));
            }
        }
    }
    for t in &all {
        if !t.wrap {
            continue;
        }
        let name = plain(&t.text);
        for piece in crate::theme::unbreakable(&name) {
            if width_of(probe, widths, piece, t.size, t.serif) > t.r.w + 1.0 {
                out.push(issue(
                    Kind::MidWord,
                    format!("{piece:?} wider than {} in {name:?}", t.r.w),
                    t.r,
                ));
            }
        }
    }
    out
}

fn touch_event(scale: f32, phase: TouchPhase, x: f32, y: f32) -> WindowEvent {
    WindowEvent::Touch(Touch {
        device_id: DeviceId::dummy(),
        phase,
        location: PhysicalPosition::new(f64::from(x * scale), f64::from(y * scale)),
        force: None,
        id: 1,
    })
}

fn cursor_event(scale: f32, x: f32, y: f32) -> WindowEvent {
    WindowEvent::CursorMoved {
        device_id: DeviceId::dummy(),
        position: PhysicalPosition::new(f64::from(x * scale), f64::from(y * scale)),
    }
}

// `frame` with an overlay painted over it.
fn render(
    gfx: &Gfx,
    frame: &image::RgbaImage,
    paint: impl FnOnce(&OverlayTarget<'_>, &mut wgpu::CommandEncoder),
) -> image::RgbaImage {
    let (w, h) = frame.dimensions();
    let tex = gfx.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("slint shot"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: gfx.surface_format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut px = frame.clone().into_raw();
    px.as_chunks_mut::<4>()
        .0
        .iter_mut()
        .for_each(|p| p.swap(0, 2));
    gfx.queue.write_texture(
        tex.as_image_copy(),
        &px,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 4),
            rows_per_image: Some(h),
        },
        tex.size(),
    );
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let t = OverlayTarget {
        device: &gfx.device,
        queue: &gfx.queue,
        view: &view,
        format: gfx.surface_format(),
        size: (w, h),
        frame: gfx.frame_rect(w, h),
    };
    let mut enc = gfx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    paint(&t, &mut enc);
    gfx.read_texture(enc, &tex)
}

// A live Slint pause card on a headless device, with the menu's own state, driven by a mock clock.
pub struct CardHarness<'a> {
    pub rig: CardRig,
    gfx: &'a Gfx,
    probe: TextProbe,
    widths: Widths,
    touch: bool,
}

impl<'a> CardHarness<'a> {
    pub fn new(
        gfx: &'a Gfx,
        s: &Settings,
        size: (u32, u32),
        safe: SafeArea,
        shot: &Shot,
    ) -> anyhow::Result<Self> {
        host::advance_mock_time(Duration::from_secs(1));
        let gpu = host::Gpu {
            instance: gfx.instance().clone(),
            device: gfx.device.clone(),
            queue: gfx.queue.clone(),
        };
        let menu = Menu::new(gpu, 0)?;
        let rig = CardRig::new(menu, s, size, safe, shot);
        let mut h = Self {
            rig,
            gfx,
            probe: TextProbe::new()?,
            widths: HashMap::new(),
            touch: shot.touch,
        };
        h.tick(16);
        h.tick(16);
        for &n in &shot.navs {
            h.nav(n);
        }
        Ok(h)
    }

    pub fn tick(&mut self, ms: u64) {
        host::advance_mock_time(Duration::from_millis(ms));
        self.rig.menu.tick();
        self.rig.pass(Vec::new());
    }

    pub fn nav(&mut self, n: Nav) {
        self.rig.pass(vec![n]);
        self.tick(16);
    }

    pub fn targets(&self, prefix: &str) -> Vec<(String, R)> {
        ids(&self.rig.menu.ui, prefix)
    }

    // The target whose name starts with `prefix`.
    pub fn target(&self, prefix: &str) -> R {
        self.targets(&format!("target|{prefix}"))
            .first()
            .unwrap_or_else(|| panic!("no target {prefix}"))
            .1
    }

    pub fn tap(&mut self, r: R) {
        let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        for (p, ms) in [(TouchPhase::Started, 40), (TouchPhase::Ended, 16)] {
            let ev = touch_event(self.rig.ppp(), p, x, y);
            self.rig.pointer(&ev);
            self.tick(ms);
        }
        self.tick(200);
    }

    pub fn click(&mut self, r: R) {
        use winit::event::{ElementState, MouseButton};
        let ev = cursor_event(self.rig.ppp(), r.x + r.w / 2.0, r.y + r.h / 2.0);
        self.rig.pointer(&ev);
        self.tick(16);
        for state in [ElementState::Pressed, ElementState::Released] {
            self.rig.pointer(&WindowEvent::MouseInput {
                device_id: DeviceId::dummy(),
                state,
                button: MouseButton::Left,
            });
            self.tick(16);
        }
        self.tick(200);
    }

    // Takes what the card asked for since the last call.
    pub fn take_done(&mut self) -> Vec<Do> {
        std::mem::take(&mut self.rig.done)
    }

    pub fn issues(&mut self) -> Vec<Issue> {
        let keys = self.rig.keys_focus();
        let mut out = common_issues(
            &self.rig.menu.ui,
            &mut self.probe,
            &mut self.widths,
            &Frame {
                scale: self.rig.ppp(),
                density: self.rig.density(),
                touch: self.touch,
                keys,
                list: None,
                rows: &[],
            },
        );
        if self.rig.is_column() {
            // 4:3: the card stays on the playfield and ends above the player, clear of the HUD.
            let (fx, fy, gs) = self.rig.frame();
            let (right, bottom) = (fx + 416.0 * gs, fy + 360.0 * gs);
            for t in texts(&self.rig.menu.ui) {
                if t.r.right() > right + 1.0 || t.r.bottom() > bottom + 1.0 {
                    out.push(issue(
                        Kind::Clipped,
                        format!("{:?} reaches the player or the HUD", plain(&t.text)),
                        t.r,
                    ));
                }
            }
        }
        out
    }

    pub fn image(&mut self, frame: &image::RgbaImage) -> image::RgbaImage {
        let menu = &mut self.rig.menu;
        render(self.gfx, frame, |t, enc| menu.paint("card", None, t, enc))
    }
}

// A live Slint settings page on a headless device, driven by a mock clock.
pub struct Harness<'a> {
    pub menu: Menu,
    pub model: Model,
    gfx: &'a Gfx,
    lang: Lang,
    probe: TextProbe,
    widths: Widths,
    pub scale: f32,
    density: f32,
    touch: bool,
}

impl<'a> Harness<'a> {
    pub fn new(
        gfx: &'a Gfx,
        s: &Settings,
        size: (u32, u32),
        safe: SafeArea,
        shot: &Shot,
    ) -> anyhow::Result<Self> {
        host::advance_mock_time(Duration::from_secs(1));
        let (model, lang, words, place) = portmenu::slint_parts(s, size, safe, shot);
        let gpu = host::Gpu {
            instance: gfx.instance().clone(),
            device: gfx.device.clone(),
            queue: gfx.queue.clone(),
        };
        let mut menu = Menu::new(gpu, 0)?;
        menu.enter(View::Settings);
        menu.page.open(shot.touch);
        menu.page.set_words(&words);
        menu.page.place(place);
        menu.page.show_tab(shot.settings.unwrap_or(0), &model, lang);
        let probe = TextProbe::new()?;
        let mut h = Self {
            menu,
            model,
            gfx,
            lang,
            probe,
            widths: HashMap::new(),
            scale: place.lay.ppp,
            density: place.lay.density,
            touch: shot.touch,
        };
        // The query instantiates the view; its scroll range reaches `PageApi` on the next tick.
        h.list_rect();
        h.tick(16);
        for &n in &shot.navs {
            h.nav(n);
        }
        h.tick(16);
        Ok(h)
    }

    pub fn tick(&mut self, ms: u64) {
        host::advance_mock_time(Duration::from_millis(ms));
        self.menu.tick();
        self.menu.page.apply(&mut self.model, self.lang);
    }

    // The first row's data and the tab strip, whose identity shows whether they were rebuilt.
    pub fn row_and_tabs(
        &self,
    ) -> (
        slint::ModelRc<slint::SharedString>,
        slint::ModelRc<slint::SharedString>,
    ) {
        use slint::Model as _;
        let api = self.menu.page.api();
        let row = api.get_rows().row_data(0).expect("a row");
        (row.pieces, api.get_tabs())
    }

    // The tab shown is opened again from the same model.
    pub fn reshow_tab(&mut self) {
        let tab = self.menu.page.tab();
        self.menu.page.show_tab(tab, &self.model, self.lang);
    }

    pub fn nav(&mut self, n: Nav) {
        self.menu.page.nav(n);
        self.tick(16);
    }

    fn event(&mut self, ev: &WindowEvent) {
        self.menu.on_event(ev);
    }

    fn touch_ev(&self, phase: TouchPhase, x: f32, y: f32) -> WindowEvent {
        touch_event(self.scale, phase, x, y)
    }

    // A finger from (x, y0) to (x, y1) in points, held still before the release, so nothing coasts.
    pub fn drag(&mut self, x: f32, y0: f32, y1: f32) {
        self.drag_xy((x, y0), (x, y1));
    }

    pub fn drag_xy(&mut self, from: (f32, f32), to: (f32, f32)) {
        self.drag_from(from, to, 0, 0.0);
    }

    // A vertical drag as fingers make it: held `hold_ms` first, wobbling sideways by up to `wobble` points.
    pub fn sloppy_drag(&mut self, x: f32, y0: f32, y1: f32, hold_ms: u64, wobble: f32) {
        self.drag_from((x, y0), (x, y1), hold_ms, wobble);
    }

    fn drag_from(&mut self, from: (f32, f32), to: (f32, f32), hold_ms: u64, wobble: f32) {
        let ev = self.touch_ev(TouchPhase::Started, from.0, from.1);
        self.event(&ev);
        self.tick(16);
        if hold_ms > 0 {
            self.tick(hold_ms);
        }
        for i in 1..=24 {
            let k = i as f32 / 24.0;
            let off = if i % 2 == 0 { wobble } else { -wobble };
            let ev = self.touch_ev(
                TouchPhase::Moved,
                from.0 + (to.0 - from.0) * k + off,
                from.1 + (to.1 - from.1) * k,
            );
            self.event(&ev);
            self.tick(16);
        }
        self.tick(200);
        let ev = self.touch_ev(TouchPhase::Ended, to.0, to.1);
        self.event(&ev);
        self.tick(16);
        self.tick(1500);
    }

    pub fn tap(&mut self, x: f32, y: f32) {
        for (p, ms) in [(TouchPhase::Started, 40), (TouchPhase::Ended, 16)] {
            let ev = self.touch_ev(p, x, y);
            self.event(&ev);
            self.tick(ms);
        }
        self.tick(200);
    }

    fn cursor_at(&mut self, x: f32, y: f32) {
        self.event(&cursor_event(self.scale, x, y));
        self.tick(16);
    }

    pub fn wheel(&mut self, x: f32, y: f32, lines: f32) {
        self.cursor_at(x, y);
        self.event(&WindowEvent::MouseWheel {
            device_id: DeviceId::dummy(),
            delta: MouseScrollDelta::LineDelta(0.0, lines),
            phase: TouchPhase::Moved,
        });
        self.tick(16);
        self.tick(400);
    }

    fn ids(&self, prefix: &str) -> Vec<(String, R)> {
        ids(&self.menu.ui, prefix)
    }

    pub fn list_rect(&self) -> R {
        self.ids("list").first().map_or(
            R {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
            |x| x.1,
        )
    }

    // Row targets (name → rect) wholly inside the list.
    pub fn visible_rows(&self) -> Vec<(String, R)> {
        let list = self.list_rect();
        self.ids("target|row|")
            .into_iter()
            .filter(|(_, r)| r.inside(list))
            .collect()
    }

    pub fn row_rect(&self, label: &str) -> Option<R> {
        self.ids("target|row|")
            .into_iter()
            .find(|(n, _)| plain(n).ends_with(&format!("|{label}")))
            .map(|x| x.1)
    }

    pub fn row_names(&self) -> Vec<String> {
        let mut v: Vec<(f32, String)> = self
            .ids("target|row|")
            .into_iter()
            .map(|(n, r)| (r.y, n))
            .collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        v.into_iter().map(|x| x.1).collect()
    }

    pub fn targets(&self, prefix: &str) -> Vec<(String, R)> {
        self.ids(prefix)
    }

    // The layout check of the frame as it stands.
    pub fn issues(&mut self) -> Vec<Issue> {
        let list = self.list_rect();
        let rows: Vec<R> = self.ids("target|row|").into_iter().map(|x| x.1).collect();
        let keys = self.page_bar();
        let mut out = common_issues(
            &self.menu.ui,
            &mut self.probe,
            &mut self.widths,
            &Frame {
                scale: self.scale,
                density: self.density,
                touch: self.touch,
                keys,
                list: Some(list),
                rows: &rows,
            },
        );
        for (n, r) in self.ids("target|row|") {
            if r.w < list.w * 0.9 {
                out.push(issue(
                    Kind::FewRows,
                    format!("{n}: row {:.0} wide in a list of {:.0}", r.w, list.w),
                    r,
                ));
            }
            let shown = r.cut(list).h / r.h.max(1.0);
            let faded = self.ids("cut").iter().any(|(_, c)| c.hits(r));
            if shown > 0.01 && shown < 0.7 && !faded {
                out.push(issue(
                    Kind::Clipped,
                    format!("{n}: half a row shows ({shown:.2})"),
                    r,
                ));
            }
        }
        if list.w < 200.0 || list.h < 100.0 {
            out.push(issue(
                Kind::FewRows,
                format!("list is {:.0}x{:.0}", list.w, list.h),
                list,
            ));
        }
        out
    }

    fn page_bar(&self) -> bool {
        use super::{Modality, Ui};
        use slint::Global;
        Ui::get(&self.menu.ui).get_modality() == Modality::Keys
    }

    // Row names reached by repeated touch drags, wheel or keys.
    pub fn reach(&mut self, how: Reach) -> BTreeSet<String> {
        let list = self.list_rect();
        let (cx, cy) = (list.x + list.w * 0.2, list.y + list.h * 0.5);
        let mut seen: BTreeSet<String> = self.visible_rows().into_iter().map(|x| x.0).collect();
        match how {
            Reach::Touch => {
                for _ in 0..40 {
                    if self.menu.page.api().get_scroll()
                        >= self.menu.page.api().get_scroll_max() - 0.5
                    {
                        break;
                    }
                    self.drag(cx, cy + list.h * 0.35, cy - list.h * 0.35);
                    seen.extend(self.visible_rows().into_iter().map(|x| x.0));
                }
            }
            Reach::Wheel => {
                for _ in 0..60 {
                    if self.menu.page.api().get_scroll()
                        >= self.menu.page.api().get_scroll_max() - 0.5
                    {
                        break;
                    }
                    self.wheel(cx, cy, -1.0);
                    seen.extend(self.visible_rows().into_iter().map(|x| x.0));
                }
            }
            Reach::Keys => {
                let n = self.row_names().len();
                self.nav(Nav::Up);
                for _ in 0..=n {
                    self.nav(Nav::Down);
                    seen.extend(self.visible_rows().into_iter().map(|x| x.0));
                }
            }
        }
        seen
    }

    pub fn image(&mut self, frame: &image::RgbaImage) -> image::RgbaImage {
        let menu = &mut self.menu;
        render(self.gfx, frame, |t, enc| {
            menu.paint("settings", None, t, enc);
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    Touch,
    Wheel,
    Keys,
}

// Layout check plus reachability by every input of one page.
pub fn check_page(
    gfx: &Gfx,
    s: &Settings,
    size: (u32, u32),
    safe: SafeArea,
    shot: &Shot,
) -> anyhow::Result<Vec<Issue>> {
    let mut h = Harness::new(gfx, s, size, safe, shot)?;
    let mut out = h.issues();
    if !shot.navs.is_empty() {
        return Ok(out);
    }
    let all = h.row_names();
    let mut ways = vec![Reach::Wheel, Reach::Keys];
    if shot.touch {
        ways.insert(0, Reach::Touch);
    }
    for how in ways {
        let mut h = Harness::new(gfx, s, size, safe, shot)?;
        let seen = h.reach(how);
        let missing: Vec<&String> = all.iter().filter(|n| !seen.contains(*n)).collect();
        if !missing.is_empty() {
            out.push(Issue {
                kind: Kind::Unreachable,
                what: format!("{how:?}: {missing:?}"),
                rect: egui::Rect::NOTHING,
            });
        }
    }
    Ok(out)
}

// CPU ms (median, p99) of a frame in which the focused row changes every frame, GPU work finished.
pub fn bench_page(
    gfx: &Gfx,
    s: &Settings,
    size: (u32, u32),
    safe: SafeArea,
    shot: &Shot,
) -> anyhow::Result<(f64, f64)> {
    let mut h = Harness::new(gfx, s, size, safe, shot)?;
    let frame = image::RgbaImage::from_pixel(size.0, size.1, image::Rgba([40, 30, 70, 255]));
    let _ = h.image(&frame);
    let (w, hh) = size;
    let tex = gfx.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bench"),
        size: wgpu::Extent3d {
            width: w,
            height: hh,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: gfx.surface_format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let mut times = Vec::with_capacity(300);
    for i in 0..300 {
        h.menu
            .page
            .nav(if i % 2 == 0 { Nav::Down } else { Nav::Up });
        let start = web_time::Instant::now();
        h.tick(16);
        let t = OverlayTarget {
            device: &gfx.device,
            queue: &gfx.queue,
            view: &view,
            format: gfx.surface_format(),
            size,
            frame: gfx.frame_rect(w, hh),
        };
        let mut enc = gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        h.menu.paint("settings", None, &t, &mut enc);
        gfx.queue.submit([enc.finish()]);
        let _ = gfx.device.poll(wgpu::PollType::wait_indefinitely());
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    Ok(crate::platform::portmenu::median_p99(&mut times))
}
