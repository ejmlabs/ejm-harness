use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{RuntimeEvent, RuntimePool, runtime::Callback};

mod hook;
mod provider;
mod tool;

pub(crate) use hook::hooks;
pub(crate) use provider::provider;
pub(crate) use tool::tool;

/// One installation revision's handlers, reached through the shared runtime pool.
#[derive(Clone)]
struct Binding {
    pool: RuntimePool,
    package_id: String,
    revision: uuid::Uuid,
}

impl Binding {
    fn callback(
        &self,
        handler: String,
        args: Value,
        context: Value,
        cancel: CancellationToken,
        events: Option<mpsc::Sender<RuntimeEvent>>,
    ) -> Callback {
        Callback {
            package_id: self.package_id.clone(),
            revision: self.revision,
            handler,
            args,
            context,
            cancel,
            events,
        }
    }
}
