//! The window's icon: winit's on Windows / X11, `xdg-toplevel-icon-v1` on Wayland (`wayland.rs`), from th08.ico.

use winit::window::{Icon, Window, WindowAttributes};

#[cfg(target_os = "linux")]
mod wayland;

const ICO: &[u8] = include_bytes!("../packaging/windows/th08.ico");

// Wayland app_id / X11 WM_CLASS: the .desktop file's name, so a desktop shell can match it to that icon.
#[cfg(target_os = "linux")]
const APP_ID: &str = "th08";

// The .ico's PNG entries as (edge, RGBA8), smallest first.
pub fn sizes() -> Vec<(u32, Vec<u8>)> {
    let entry = |i: usize| -> Option<(u32, Vec<u8>)> {
        let e = ICO.get(6 + 16 * i..22 + 16 * i)?;
        let len = u32::from_le_bytes(e[8..12].try_into().ok()?) as usize;
        let off = u32::from_le_bytes(e[12..16].try_into().ok()?) as usize;
        let img =
            image::load_from_memory_with_format(ICO.get(off..off + len)?, image::ImageFormat::Png)
                .inspect_err(|e| log::warn!("window icon: {e}"))
                .ok()?
                .into_rgba8();
        (img.width() == img.height()).then(|| (img.width(), img.into_raw()))
    };
    let count = ICO
        .get(4..6)
        .map_or(0, |n| u16::from_le_bytes([n[0], n[1]]));
    let mut out: Vec<_> = (0..usize::from(count)).filter_map(entry).collect();
    out.sort_by_key(|&(edge, _)| edge);
    out.dedup_by_key(|&mut (edge, _)| edge);
    out
}

pub fn attrs(attrs: WindowAttributes) -> WindowAttributes {
    #[cfg(target_os = "linux")]
    let attrs = {
        let attrs =
            winit::platform::wayland::WindowAttributesExtWayland::with_name(attrs, APP_ID, APP_ID);
        winit::platform::x11::WindowAttributesExtX11::with_name(attrs, APP_ID, APP_ID)
    };
    let icon = sizes().pop().and_then(|(edge, rgba)| {
        Icon::from_rgba(rgba, edge, edge)
            .inspect_err(|e| log::warn!("window icon: {e}"))
            .ok()
    });
    attrs.with_window_icon(icon)
}

// Wayland ignores winit's icon: hands the window its own one, if the compositor has the protocol.
#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
pub fn created(window: &Window) {
    #[cfg(target_os = "linux")]
    if let Some(toplevel) = winit::platform::wayland::WindowExtWayland::xdg_toplevel(window) {
        use winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
        if let Ok(RawDisplayHandle::Wayland(display)) = window.display_handle().map(|h| h.as_raw())
        {
            match wayland::set(display.display, toplevel, &sizes()) {
                Ok(true) => log::info!("window icon: set through xdg-toplevel-icon-v1"),
                Ok(false) => log::info!(
                    "window icon: no xdg-toplevel-icon-v1, the compositor takes it from th08.desktop"
                ),
                Err(e) => log::warn!("window icon: {e}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ico_has_every_size() {
        let sizes = super::sizes();
        let edges: Vec<u32> = sizes.iter().map(|&(e, _)| e).collect();
        assert_eq!(edges, [16, 24, 32, 48, 64, 128, 256]);
        for (edge, rgba) in &sizes {
            assert_eq!(rgba.len(), (edge * edge * 4) as usize);
            assert!(
                rgba.as_chunks::<4>().0.iter().any(|p| p[3] == 255),
                "{edge}: fully transparent"
            );
        }
    }

    #[test]
    fn attrs_carry_the_largest_icon() {
        let attrs = super::attrs(winit::window::Window::default_attributes());
        assert!(attrs.window_icon.is_some());
    }
}
