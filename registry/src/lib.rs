//! Cloudflare registry for immutable ejm extension packages.

pub mod policy;

#[cfg(target_arch = "wasm32")]
mod edge;
