use anyhow::Result;
use rusqlite::{OptionalExtension, Transaction};

use super::Store;

impl Store {
    /// Record the provider binding. Changing it drops stored provider continuations, which
    /// only the same provider may reuse. Readable history and tool evidence stay.
    pub async fn configure_provider_binding(&self, binding: Option<String>) -> Result<()> {
        self.call(move |db| {
            let tx = db.transaction()?;
            let current: Option<String> = tx
                .query_row(
                    "SELECT value FROM settings WHERE key='provider_binding'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            if current != binding {
                drop_continuations(&tx)?;
                save_binding(&tx, binding)?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }
}

fn drop_continuations(tx: &Transaction<'_>) -> Result<()> {
    tx.execute(
        "UPDATE messages SET payload=json_remove(payload,'$.continuation'),bytes=length(CAST(json_remove(payload,'$.continuation') AS BLOB)) WHERE json_type(payload,'$.continuation') IS NOT NULL",
        [],
    )?;
    tx.execute(
        "UPDATE tool_views SET payload=json_remove(payload,'$.continuation'),bytes=length(CAST(json_remove(payload,'$.continuation') AS BLOB)) WHERE payload IS NOT NULL AND json_type(payload,'$.continuation') IS NOT NULL",
        [],
    )?;
    Ok(())
}

fn save_binding(tx: &Transaction<'_>, binding: Option<String>) -> Result<()> {
    if let Some(binding) = binding {
        tx.execute(
            "INSERT INTO settings(key,value) VALUES('provider_binding',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [binding],
        )?;
    } else {
        tx.execute("DELETE FROM settings WHERE key='provider_binding'", [])?;
    }
    Ok(())
}
