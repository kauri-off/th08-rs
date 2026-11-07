//! The window's own icon through `xdg-toplevel-icon-v1` over winit's raw `wl_display` / `xdg_toplevel`.
#![allow(unsafe_code, reason = "winit's raw wl_display and xdg_toplevel")]

use std::ffi::c_void;
use std::fs::File;
use std::io::Write;
use std::os::fd::AsFd;
use std::ptr::NonNull;

use wayland_backend::client::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_shm::{self, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, delegate_noop};
use wayland_protocols::xdg::shell::client::xdg_toplevel::XdgToplevel;
use wayland_protocols::xdg::toplevel_icon::v1::client::xdg_toplevel_icon_manager_v1::XdgToplevelIconManagerV1;
use wayland_protocols::xdg::toplevel_icon::v1::client::xdg_toplevel_icon_v1::XdgToplevelIconV1;

struct State;

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(State: ignore WlShm);
delegate_noop!(State: WlShmPool);
delegate_noop!(State: ignore WlBuffer);
delegate_noop!(State: ignore XdgToplevelIconManagerV1);
delegate_noop!(State: XdgToplevelIconV1);

// wl_shm's ARGB8888: little-endian BGRA, premultiplied.
fn argb(rgba: &[u8]) -> impl Iterator<Item = u8> + '_ {
    rgba.as_chunks::<4>().0.iter().flat_map(|p| {
        let a = u16::from(p[3]);
        let m = |c: u8| u8::try_from(u16::from(c) * a / 255).unwrap_or(u8::MAX);
        [m(p[2]), m(p[1]), m(p[0]), p[3]]
    })
}

// `Ok(false)`: the compositor has no `xdg-toplevel-icon-v1`.
pub fn set(
    display: NonNull<c_void>,
    toplevel: NonNull<c_void>,
    sizes: &[(u32, Vec<u8>)],
) -> Result<bool, String> {
    let e = |e: &dyn std::fmt::Display| e.to_string();
    // SAFETY: winit's display and toplevel outlive this call; the queue is never dropped (below).
    let conn =
        Connection::from_backend(unsafe { Backend::from_foreign_display(display.as_ptr().cast()) });
    let toplevel =
        unsafe { ObjectId::from_ptr(XdgToplevel::interface(), toplevel.as_ptr().cast()) }
            .and_then(|id| XdgToplevel::from_id(&conn, id))
            .map_err(|x| e(&x))?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn).map_err(|x| e(&x))?;
    let qh = queue.handle();
    let Ok(manager) = globals.bind::<XdgToplevelIconManagerV1, _, _>(&qh, 1..=1, ()) else {
        return Ok(false);
    };
    let shm: WlShm = globals.bind(&qh, 1..=1, ()).map_err(|x| e(&x))?;

    let data: Vec<u8> = sizes.iter().flat_map(|(_, rgba)| argb(rgba)).collect();
    let fd = rustix::fs::memfd_create("th08-icon", rustix::fs::MemfdFlags::CLOEXEC)
        .map_err(|x| e(&x))?;
    let mut file = File::from(fd);
    file.write_all(&data).map_err(|x| e(&x))?;
    let len = i32::try_from(data.len()).map_err(|x| e(&x))?;
    let pool = shm.create_pool(file.as_fd(), len, &qh, ());
    let icon = manager.create_icon(&qh, ());
    let mut offset = 0i32;
    let mut buffers = Vec::with_capacity(sizes.len());
    for (edge, _) in sizes {
        let side = i32::try_from(*edge).map_err(|x| e(&x))?;
        let buffer = pool.create_buffer(
            offset,
            side,
            side,
            side * 4,
            wl_shm::Format::Argb8888,
            &qh,
            (),
        );
        icon.add_buffer(&buffer, 1);
        buffers.push(buffer);
        offset += side * side * 4;
    }
    manager.set_icon(&toplevel, Some(&icon));
    // The buffers must outlive the icon object (`no_buffer` error otherwise).
    icon.destroy();
    for buffer in buffers {
        buffer.destroy();
    }
    pool.destroy();
    manager.destroy();
    queue.roundtrip(&mut State).map_err(|x| e(&x))?;
    // The registry and wl_shm stay attached to this queue: destroying it under them would leave them dangling.
    std::mem::forget(queue);
    Ok(true)
}

#[cfg(test)]
mod tests {
    #[test]
    fn argb_is_premultiplied_bgra() {
        let px: Vec<u8> = super::argb(&[255, 128, 0, 255, 200, 100, 50, 128, 9, 9, 9, 0]).collect();
        assert_eq!(px, [0, 128, 255, 255, 25, 50, 100, 128, 0, 0, 0, 0]);
    }
}
