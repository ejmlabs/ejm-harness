use std::{
    io::{Read, Write},
    path::PathBuf,
    time::Duration,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::var("EJM_HARNESS_EXEC_FIXTURE")?;
    let pid_file = std::env::var_os("EJM_HARNESS_EXEC_PID_FILE").map(PathBuf::from);
    if let Some(path) = std::env::var_os("EJM_HARNESS_EXEC_CHILD_PID_FILE") {
        std::fs::write(path, format!("{}\n", std::process::id()))?;
    }
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
        "descendant" | "orphan" | "many-descendants" => descendants(&mode, pid_file)?,
        _ => std::process::exit(2),
    }
    Ok(())
}

fn descendants(mode: &str, pid_file: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let count = if mode == "many-descendants" { 5 } else { 1 };
    let mut children = Vec::new();
    let mut pids = vec![std::process::id()];
    for index in 0..count {
        let mut command = std::process::Command::new(std::env::current_exe()?);
        command.env("EJM_HARNESS_EXEC_FIXTURE", "hold");
        if mode == "many-descendants"
            && let Some(path) = &pid_file
        {
            std::fs::write(path.with_extension(format!("{index}.pending")), "")?;
            command.env(
                "EJM_HARNESS_EXEC_CHILD_PID_FILE",
                path.with_extension(format!("{index}.child")),
            );
        }
        let child = command.spawn()?;
        if mode == "many-descendants"
            && let Some(path) = &pid_file
        {
            std::fs::write(
                path.with_extension(format!("{index}.parent")),
                format!("{}\n", child.id()),
            )?;
        }
        pids.push(child.id());
        children.push(child);
        if mode == "many-descendants"
            && pids.len() == 2
            && let Some(path) = &pid_file
        {
            // Publish before the next child can trigger the process limit.
            std::fs::write(path, format!("{} {}\n", pids[0], pids[1]))?;
        }
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
