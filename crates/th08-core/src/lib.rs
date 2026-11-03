//! th08 game logic core: platform-free `World` state, the `Ctx` frame driver and every ported subsystem
//! (rewrite step 10, STEP10_CRATES.md §1). No renderer, audio device, window, file system or clock: those are the
//! `Renderer` / `AudioOut` / `Host` / `DataSource` traits implemented by th08-render, th08-audio and the app.
// Logic transcendentals go through `fmath` (05 §10); clippy.toml lists the banned std methods.
#![deny(clippy::disallowed_methods)]
#![forbid(unsafe_code)]

pub mod anm;
pub mod anmdebug;
pub mod anmmgr;
pub mod anmvm;
pub mod archive;
pub mod ascii;
pub mod assets;
pub mod bomb;
pub mod bullet;
pub mod camera;
pub mod chain;
pub mod config;
pub mod content;
pub mod coreconfig;
pub mod d3dx;
pub mod drawfmt;
pub mod drawlist;
pub mod ecl;
pub mod eclvm;
pub mod effect;
pub mod ending;
pub mod enemy;
pub mod fmath;
pub mod game;
pub mod gamemgr;
pub mod gui;
pub mod host;
pub mod hud;
pub mod input;
pub mod item;
pub mod menu;
pub mod menulist;
pub mod msg;
pub mod musicroom;
pub mod player;
pub mod pshot;
pub mod replay;
pub mod result;
pub mod rng;
pub mod scoredat;
pub mod sht;
pub mod sound;
pub mod stage;
pub mod stdfile;
pub mod supervisor;
pub mod task;
pub mod testdata;
pub mod trace;
pub mod world;
#[cfg(test)]
mod worldclone;
