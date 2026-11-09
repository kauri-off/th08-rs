//! th08 renderer on wgpu: draw / resource commands and text; debug-build `TH08_DEVICE_LOSS=<n>` rebuilds the device at the n-th Present.

pub mod gfx;
pub mod scene_draw;
mod scene_gpu;
pub mod text;
pub mod textdb;
