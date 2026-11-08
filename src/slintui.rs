//! The port's Slint UI next to egui. Pin: slint, slint-build, i-slint-backend-testing and (Android, whose public femtovg module slint compiles out) i-slint-renderer-femtovg are `=1.18.1` with `unstable-wgpu-30` on our wgpu 30; those four and wgpu move together.
slint::include_modules!();

pub mod host;
pub mod input;
pub mod settings_page;
pub mod text;

#[cfg(feature = "ui-check")]
pub mod check;
