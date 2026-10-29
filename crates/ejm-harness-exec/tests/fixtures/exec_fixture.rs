use std::{
    io::{Read, Write},
    path::PathBuf,
    time::Duration,
};

fn main() -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    if let Some(result) = ejm_harness_exec::native::dispatch_helper() {
        return result.map_err(Into::into);
    }
    let mode = std::env::var("EJM_HARNESS_EXEC_FIXTURE")?;
    let pid_file = std::env::var_os("EJM_HARNESS_EXEC_PID_FILE").map(PathBuf::from);
    match mode.as_str() {
        "private-input" => {
            let err = std::thread::spawn(|| -> std::io::Result<()> {
                let mut stderr = std::io::stderr().lock();
                stderr.write_all(&vec![b'e'; 1024 * 1024])?;
                stderr.write_all(b"\nSTDERR_END\n")?;
                Ok(())
            });
            std::io::stdout().write_all(&vec![b'o'; 1024 * 1024])?;
            let mut input = Vec::new();
            std::io::stdin().read_to_end(&mut input)?;
            err.join()
                .map_err(|_| std::io::Error::other("fixture writer panicked"))??;
            println!(
                "INPUT_LEN={} PATH_REMOVED={}",
                input.len(),
                std::env::var_os("PATH").is_none()
            );
        }
        "blocked-input" => {
            if let Some(path) = pid_file {
                std::fs::write(path, format!("{}\n", std::process::id()))?;
            }
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        "noisy" => {
            let err = std::thread::spawn(|| -> std::io::Result<()> {
                let mut stderr = std::io::stderr().lock();
                stderr.write_all(&vec![b'e'; 1024 * 1024])?;
                stderr.write_all(b"\nSTDERR_END\n")?;
                Ok(())
            });
            std::io::stdout().write_all(&vec![b'o'; 1024 * 1024])?;
            err.join()
                .map_err(|_| std::io::Error::other("fixture writer panicked"))??;
            std::io::stdout().write_all(b"\nSTDOUT_END\n")?;
        }
        "binary" => std::io::stdout().write_all(&vec![0xff_u8; 1024 * 1024])?,
        "ready" => {
            if let Some(path) = pid_file {
                std::fs::write(path, format!("{}\n", std::process::id()))?;
            }
        }
        "hold" => loop {
            std::thread::sleep(Duration::from_secs(1));
        },
        "hold-report" => {
            let path = pid_file.ok_or("fixture requires a child PID file")?;
            std::fs::write(path, format!("{}\n", std::process::id()))?;
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        "spawn-paused" => {
            let path = pid_file.ok_or("fixture requires a PID file")?;
            let directory = path.parent().ok_or("fixture PID file has no parent")?;
            let mut children = Vec::new();
            let mut pids = vec![std::process::id()];
            for index in 0..5 {
                let child = std::process::Command::new(std::env::current_exe()?)
                    .env("EJM_HARNESS_EXEC_FIXTURE", "hold-report")
                    .env(
                        "EJM_HARNESS_EXEC_PID_FILE",
                        directory.join(format!("child-{index}")),
                    )
                    .spawn()?;
                if index == 1 {
                    std::fs::write(
                        &path,
                        format!(
                            "{}\n",
                            pids.iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(" ")
                        ),
                    )?;
                    while !directory.join("resume").exists() {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                pids.push(child.id());
                children.push(child);
            }
            for mut child in children {
                let _ = child.wait();
            }
        }
        "descendant" | "orphan" | "many-descendants" => descendants(&mode, pid_file)?,
        _ => std::process::exit(2),
    }
    Ok(std::process::ExitCode::SUCCESS)
}

fn descendants(mode: &str, pid_file: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let count = if mode == "many-descendants" { 5 } else { 1 };
    let mut children = Vec::new();
    let mut pids = vec![std::process::id()];
    for _ in 0..count {
        let mut command = std::process::Command::new(std::env::current_exe()?);
        command.env("EJM_HARNESS_EXEC_FIXTURE", "hold");
        let child = command.spawn()?;
        pids.push(child.id());
        children.push(child);
    }
    if mode != "many-descendants"
        && let Some(path) = pid_file
    {
        std::fs::write(
            path,
            format!(
                "{}\n",
                pids.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        )?;
    }
    if mode != "orphan" {
        for mut child in children {
            let _ = child.wait();
        }
    }
    Ok(())
}
