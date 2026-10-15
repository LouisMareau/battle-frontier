//! The game's 3D renderer on wgpu (WebGPU, or WebGL 2 where a browser has no WebGPU):
//! world maps and the placeholder battle maps, sprites as billboards, effects, the sun's shadow,
//! day and night. It knows nothing of battles: the game hands it a `Frame` each frame.

pub mod map;
pub mod scene;

pub use glam;
pub use wgpu;
