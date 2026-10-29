#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
    };

    use anyhow::{Context, ensure};
    struct Service(Child);
    impl Drop for Service {
        fn drop(&mut self) {
            drop(self.0.stdin.take());
            let _ = self.0.wait();
        }
    }

    if std::env::args_os().nth(1).is_some() {
        return isolated_round_trip();
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
    let result = Command::new(std::env::current_exe()?)
        .arg("isolated-child")
        .env("DBUS_SESSION_BUS_ADDRESS", address.trim_end())
        .status()
        .context("run isolated native key fixture")?;
    drop(wrapper.0.stdin.take());
    let status = wrapper.0.wait()?;
    ensure!(result.success(), "isolated native key fixture failed");
    ensure!(status.success(), "isolated service cleanup failed");
    Ok(())
}

#[cfg(target_os = "linux")]
fn isolated_round_trip() -> anyhow::Result<()> {
    use std::os::unix::fs::MetadataExt;

    use anyhow::{Context, ensure};
    use aws_lc_rs::digest::{SHA256, digest};
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use ejm_harness_auth::{SecretId, SecretString, SecretVault};
    use keyring_core::api::CredentialStoreApi;
    use tokio_util::sync::CancellationToken;

    let parent = rustix::process::getppid().context("fixture has no parent")?;
    let parent_exe = std::fs::metadata(format!("/proc/{}/exe", parent.as_raw_pid()))?;
    let self_exe = std::fs::metadata("/proc/self/exe")?;
    ensure!(
        (parent_exe.dev(), parent_exe.ino()) == (self_exe.dev(), self_exe.ino()),
        "fixture child must be started by its isolated parent"
    );
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    tokio::runtime::Builder::new_current_thread()
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
        })
}

#[cfg(not(target_os = "linux"))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("native key fixture requires Linux")
}
