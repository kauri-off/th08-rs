//! th08 renderer: executes the core's draw / resource commands on wgpu (`Gfx: Renderer`) and draws text
//! (STEP10_CRATES.md §1).
//! Debug env `TH08_DEVICE_LOSS=<n>`: the device is destroyed at the n-th Present and rebuilt (ARCH_D_MOBILE §5 check).

pub mod gfx;
pub mod text;
pub mod textdb;
