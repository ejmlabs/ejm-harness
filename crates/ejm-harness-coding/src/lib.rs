//! Native coding tools with explicit effect grants. Shell access is a separate host grant.
mod edit;
mod list;
mod native;
mod path;
mod read;
mod search;
mod shell;

use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use ejm_harness::Tool;

use native::{Kind, NativeTool};
use shell::ShellTool;

/// Visible output limit shared by file reads, listings, and search results.
const MAX_RETRIEVAL_BYTES: usize = 14 * 1024;

#[derive(Clone)]
pub struct CodingConfig {
    pub allow_write: bool,
    pub allow_shell: bool,
    pub artifact_dir: PathBuf,
}

impl Default for CodingConfig {
    fn default() -> Self {
        Self {
            allow_write: false,
            allow_shell: false,
            artifact_dir: PathBuf::from(".ejm-harness/artifacts"),
        }
    }
}

/// Registering these tools grants their effects. No repository file can add a grant.
///
/// # Errors
/// Reserved for invalid coding tool configuration.
pub fn tools(config: CodingConfig) -> Result<Vec<Arc<dyn Tool>>> {
    let mut result: Vec<Arc<dyn Tool>> = vec![
        Arc::new(NativeTool(Kind::Read)),
        Arc::new(NativeTool(Kind::List)),
        Arc::new(NativeTool(Kind::Search)),
    ];
    if config.allow_write {
        result.extend([
            Arc::new(NativeTool(Kind::Write)) as Arc<dyn Tool>,
            Arc::new(NativeTool(Kind::Patch)),
        ]);
    }
    if config.allow_shell {
        result.push(Arc::new(ShellTool {
            artifact_dir: config.artifact_dir,
        }));
    }
    Ok(result)
}

/// Output lines that stop growing before their newline-joined text passes [`MAX_RETRIEVAL_BYTES`].
#[derive(Default)]
struct BoundedLines {
    lines: Vec<String>,
    bytes: usize,
}

impl BoundedLines {
    /// Adds the line unless it would push the joined output past the limit.
    fn push(&mut self, line: String) -> bool {
        let bytes = line.len() + usize::from(!self.lines.is_empty());
        if self.bytes + bytes > MAX_RETRIEVAL_BYTES {
            return false;
        }
        self.bytes += bytes;
        self.lines.push(line);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ejm_harness::Effect;

    #[test]
    fn effect_grants_are_explicit() -> Result<()> {
        let read = tools(CodingConfig::default())?;
        assert!(read.iter().all(|t| t.definition().effect == Effect::Read));
        assert_eq!(read.len(), 3);
        Ok(())
    }
}
