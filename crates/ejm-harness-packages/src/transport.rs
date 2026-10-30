use anyhow::Result;
use serde_json::Value;
use tokio::{
    sync::{OwnedSemaphorePermit, mpsc, oneshot},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

use crate::runtime::RuntimeEvent;

mod actor;
mod host;

pub(crate) use host::Host;

struct Frame {
    line: String,
    _bytes: OwnedSemaphorePermit,
}

struct Pending {
    reply: oneshot::Sender<Result<Value>>,
    events: Option<mpsc::Sender<RuntimeEvent>>,
    cancel: CancellationToken,
    deadline: Instant,
    cancelled_at: Option<Instant>,
    failure: Option<String>,
    event_count: usize,
    event_bytes: usize,
    _slot: OwnedSemaphorePermit,
    _retained: Box<dyn Send>,
}

struct Request {
    id: String,
    method: &'static str,
    params: Value,
    pending: Pending,
}
