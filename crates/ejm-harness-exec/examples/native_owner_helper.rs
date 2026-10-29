//! Minimal helper entry for embedders' native integration fixtures.

use std::process::ExitCode;

use anyhow::{Context, Result};

fn main() -> Result<ExitCode> {
    ejm_harness_exec::native::dispatch_helper()
        .context("expected private native helper invocation")?
}
