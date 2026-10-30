//! Authentication stays inside the unmodified official Claude Code client.

mod client;
mod process;
mod process_group;
mod program;
mod status;

pub use client::ClaudeCodeClient;
pub use program::{CLAUDE_CODE_ENV_OVERRIDES, resolve_claude_code_program};
pub use status::ClaudeCodeStatus;

/// Largest account status or captured client output, in bytes.
const MAX_STATUS_BYTES: usize = 16 * 1024;
