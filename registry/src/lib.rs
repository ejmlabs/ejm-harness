//! Cloudflare registry for immutable Gerry extension packages.

pub mod policy;

#[cfg(target_arch = "wasm32")]
mod edge;
