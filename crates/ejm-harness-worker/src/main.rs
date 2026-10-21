fn main() -> anyhow::Result<std::process::ExitCode> {
    if let Some(result) = ejm_harness::ejm_harness_exec::native::dispatch_helper() {
        return result;
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        ejm_harness_worker::serve(
            tokio::io::BufReader::new(tokio::io::stdin()),
            tokio::io::stdout(),
        )
        .await
    });

    // Tokio's stdin read cannot be cancelled. The harness has finished cleanup,
    // so runtime shutdown must not wait for the host to close idle stdin.
    runtime.shutdown_background();
    result.map(|()| std::process::ExitCode::SUCCESS)
}
