fn main() -> anyhow::Result<()> {
    let schema = serde_json::to_string_pretty(&ejm_harness_worker::protocol::schema())?;
    if let Some(path) = std::env::args_os().nth(1) {
        std::fs::write(path, schema + "\n")?;
    } else {
        println!("{schema}");
    }
    Ok(())
}
