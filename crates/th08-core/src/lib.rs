//! th08 game logic core: platform-free `World` state, the `Ctx` frame driver and every ported subsystem.
// Logic transcendentals go through `fmath` (05 §10); clippy.toml lists the banned std methods and the
// interior-mutability types, so an emit holding `&World` cannot change logic state.
#![deny(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::disallowed_macros
)]
#![forbid(unsafe_code)]

pub mod anmdebug;
pub mod anmmgr;
pub mod anmvm;
pub mod ascii;
pub mod assets;
pub mod bomb;
pub mod builtin;
pub mod bullet;
pub mod camera;
pub mod chain;
pub mod config;
pub mod content;
pub mod coreconfig;
pub mod d3dx;
pub mod drawfmt;
pub mod drawlist;
pub mod eclvm;
pub mod effect;
mod emit;
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
pub mod sound;
pub mod stage;
pub mod supervisor;
pub mod task;
pub mod testdata;
pub mod textref;
pub mod trace;
pub mod types;
mod vanilla;
pub mod world;
#[cfg(test)]
mod worldclone;
