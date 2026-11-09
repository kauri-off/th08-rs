//! The port menu's single Slint window: one renderer and target texture, with the card and settings as views in it.
use super::card::Card;
use super::host::{self, Adapter, Gpu, Surface};
use super::input::{Pointer, Pointing};
use super::settings_page::Page;
use super::{MenuRoot, Modality, Ui};
use slint::{ComponentHandle, Global};
use std::rc::Rc;
use th08_render::gfx::OverlayTarget;
use winit::event::WindowEvent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Card,
    Settings,
}

pub struct Menu {
    pub ui: MenuRoot,
    pub card: Card,
    pub page: Page,
    surface: Surface,
    adapter: Rc<Adapter>,
    pointer: Pointer,
    view: Option<View>,
    generation: u32,
}

impl Menu {
    pub fn new(gpu: Gpu, generation: u32) -> Result<Self, slint::PlatformError> {
        host::use_gpu(gpu.clone())?;
        let ui = MenuRoot::new()?;
        ui.show()?;
        let adapter = host::take_adapter().ok_or(slint::PlatformError::NoPlatform)?;
        Ok(Self {
            card: Card::new(ui.clone_strong(), adapter.clone()),
            page: Page::new(ui.clone_strong(), adapter.clone()),
            surface: Surface::new(adapter.clone(), gpu),
            ui,
            adapter,
            pointer: Pointer::default(),
            view: None,
            generation,
        })
    }

    pub fn generation(&self) -> u32 {
        self.generation
    }

    pub fn adapter(&self) -> &Rc<Adapter> {
        &self.adapter
    }

    // A new menu session on the kept window: no finger or hover carries over from the last one.
    pub fn session_begin(&mut self) {
        self.pointer = Pointer::default();
        self.adapter
            .dispatch(slint::platform::WindowEvent::PointerExited);
    }

    // Shows `v`; true when the view changed, so the caller fills it.
    pub fn enter(&mut self, v: View) -> bool {
        if self.view == Some(v) {
            return false;
        }
        self.view = Some(v);
        self.ui.set_view(match v {
            View::Card => 0,
            View::Settings => 1,
        });
        true
    }

    // Pointer input; its kind, None when the event is not pointer input or the window has no size yet.
    pub fn on_event(&mut self, ev: &WindowEvent) -> Option<Pointing> {
        let scale = self.adapter.scale();
        if scale <= 0.0 {
            return None;
        }
        self.pointer.event(&self.adapter, ev, scale)
    }

    pub fn set_modality(&self, m: crate::menu_model::Modality) {
        use crate::menu_model::Modality as M;
        Ui::get(&self.ui).set_modality(match m {
            M::Keys => Modality::Keys,
            M::Pointer => Modality::Mouse,
            M::Touch => Modality::Touch,
        });
    }

    // Runs timers; how soon the next repaint is due.
    pub fn tick(&self) -> Option<std::time::Duration> {
        if host::tick(&self.adapter) {
            return Some(std::time::Duration::from_millis(16));
        }
        host::next_timer()
    }

    pub fn paint(
        &mut self,
        screen: &'static str,
        tab: Option<usize>,
        t: &OverlayTarget<'_>,
        enc: &mut wgpu::CommandEncoder,
    ) {
        self.surface.label(screen, tab);
        self.surface.paint(t, enc);
    }
}
