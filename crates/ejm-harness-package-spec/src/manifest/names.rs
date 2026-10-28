use anyhow::{Result, ensure};

const WINDOWS_RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Validate the registry's ASCII publisher and package namespace.
///
/// # Errors
/// Requires a scoped name such as `@publisher/package` with bounded lowercase components.
pub fn validate_name(name: &str) -> Result<()> {
    let Some((publisher, package)) = name
        .strip_prefix('@')
        .and_then(|value| value.split_once('/'))
    else {
        anyhow::bail!("package name must be @publisher/package");
    };
    for component in [publisher, package] {
        ensure!(
            !component.is_empty() && component.len() <= 48,
            "invalid package name length"
        );
        ensure!(
            component.as_bytes()[0].is_ascii_lowercase(),
            "package name must start with a letter"
        );
        ensure!(
            component
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
            "package names use lowercase letters, digits, and hyphens"
        );
        ensure!(
            !component.ends_with('-'),
            "package name cannot end in a hyphen"
        );
    }
    Ok(())
}

/// Validate a portable archive path before creating any files.
///
/// # Errors
/// Rejects absolute paths, traversal, hidden components, backslashes, and Windows aliases.
pub fn validate_relative_path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty() && path.len() <= 240 && !path.contains(['\\', ':', '\0']),
        "invalid package path"
    );
    for part in path.split('/') {
        ensure!(
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.starts_with('.')
                && !part.ends_with(['.', ' ']),
            "unsafe package path"
        );
        ensure!(
            part.chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_ .@+".contains(character)),
            "package path contains unsupported characters"
        );
        let stem = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        ensure!(
            !WINDOWS_RESERVED.contains(&stem.as_str()),
            "reserved package path"
        );
    }
    Ok(())
}

pub(super) fn validate_identifier(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')),
        "invalid export identifier"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_windows_names_are_rejected_in_any_case_with_any_extension() {
        for path in ["con", "src/Nul.txt", "lpt9.tar.gz"] {
            assert!(validate_relative_path(path).is_err(), "{path}");
        }
        for path in ["console.js", "src/lpt10.txt", "com0"] {
            assert!(validate_relative_path(path).is_ok(), "{path}");
        }
    }
}
