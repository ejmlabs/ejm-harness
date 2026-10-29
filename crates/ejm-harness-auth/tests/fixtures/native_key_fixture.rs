#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
    };

    use anyhow::{ensure, Context};
    use aws_lc_rs::digest::{digest, SHA256};
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use ejm_harness_auth::{SecretId, SecretString, SecretVault};
    use keyring_core::api::CredentialStoreApi;
    use tokio_util::sync::CancellationToken;

    struct Service(Child);
    impl Drop for Service {
        fn drop(&mut self) {
            drop(self.0.stdin.take());
            let _ = self.0.wait();
        }
    }

    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut wrapper = Service(
        Command::new(project.join("scripts/with-secret-service"))
            .args([
                "sh",
                "-c",
                "printf '%s\\n' \"$DBUS_SESSION_BUS_ADDRESS\"; cat >/dev/null",
            ])
            .env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .context("start isolated Secret Service")?,
    );
    let mut address = String::new();
    BufReader::new(wrapper.0.stdout.take().context("missing service output")?)
        .read_line(&mut address)?;
    ensure!(
        address.trim_end() == format!("unix:path=/proc/{}/fd/3/b", wrapper.0.id()),
        "isolated service did not provide its bus"
    );
    std::env::set_var("DBUS_SESSION_BUS_ADDRESS", address.trim_end());

    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let root = tempfile::tempdir_in(project.join("target"))?;
            let path = root.path().join("native-key-fixture");
            let vault = SecretVault::open(&path)?;
            let identity = URL_SAFE_NO_PAD.encode(
                digest(
                    &SHA256,
                    std::fs::canonicalize(&path)?.as_os_str().as_encoded_bytes(),
                )
                .as_ref(),
            );
            let id = SecretId::new(
                "fixture-product",
                "api-key",
                "https://one.example/account-a",
            )?;
            let result = async {
                let mut transaction = vault.lock(CancellationToken::new()).await?;
                transaction.set(id.clone(), SecretString::new("nonsecret-native-fixture"))?;
                transaction.commit()?;
                drop(transaction);
                let transaction = vault.lock(CancellationToken::new()).await?;
                ensure!(
                    transaction.get(&id).is_some(),
                    "native fixture did not reopen"
                );
                Ok::<_, anyhow::Error>(())
            }
            .await;
            let cleanup = dbus_secret_service_keyring_store::Store::new()
                .and_then(|store| store.build("com.ejmlabs.harness.vault.v1", &identity, None))
                .and_then(|entry| entry.delete_credential());
            result?;
            cleanup.map_err(|_| anyhow::anyhow!("native fixture key cleanup failed"))?;
            Ok::<_, anyhow::Error>(())
        });
    drop(wrapper.0.stdin.take());
    let status = wrapper.0.wait()?;
    result?;
    ensure!(status.success(), "isolated service cleanup failed");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("native key fixture requires Linux")
}
