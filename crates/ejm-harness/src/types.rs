mod event;
mod message;
mod model;
mod session;
mod tool;

pub use event::{Event, EventKind};
pub use message::{Message, ModelRequest, Role};
pub use model::{ModelDelta, ModelResponse, ProviderContinuation, Usage};
pub use session::{HarnessSnapshot, RunStatus, RunSummary, Session, SessionOptions, SessionStatus};
pub use tool::{Effect, ToolCall, ToolDefinition, ToolOutput};

pub(crate) fn timestamp_ms() -> u64 {
    u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

pub(crate) fn truncate_utf8(value: &mut String, max: usize) -> bool {
    if value.len() <= max {
        return false;
    }
    let mut end = max.min(value.len());
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    true
}

#[cfg(test)]
mod tests {
    use super::truncate_utf8;

    #[test]
    fn truncation_stops_on_a_char_boundary() {
        let mut text = "aé".to_owned();
        assert!(truncate_utf8(&mut text, 2));
        assert_eq!(text, "a");
        assert!(!truncate_utf8(&mut text, 2));
    }
}
