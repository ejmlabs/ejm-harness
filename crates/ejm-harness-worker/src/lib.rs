//! A bounded, bidirectional worker for hosts that use the reusable Rust harness.
pub mod protocol;
mod server;

pub use server::serve;
