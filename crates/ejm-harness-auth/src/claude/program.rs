use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

/// Resolve the official client without a shell. An explicit override is unchanged.
/// Otherwise prefer the native install in the user's `.local/bin`, then `PATH`.
/// Windows automatic lookup requires `claude.exe`, so batch shims cannot win.
/// A missing client retains its native command name for the normal launch error.
#[must_use]
pub fn resolve_claude_code_program(configured: Option<&Path>) -> PathBuf {
    configured.map_or_else(
        || {
            discover_program(
                std::env::home_dir().as_deref(),
                std::env::var_os("PATH").as_deref(),
            )
        },
        Path::to_path_buf,
    )
}

fn discover_program(home: Option<&Path>, paths: Option<&OsStr>) -> PathBuf {
    let name = if cfg!(windows) {
        "claude.exe"
    } else {
        "claude"
    };
    home.and_then(|home| which::which(home.join(".local").join("bin").join(name)).ok())
        .or_else(|| which::which_in_global(name, paths).ok()?.next())
        .unwrap_or_else(|| PathBuf::from(name))
}

/// Ambient credentials and routing overrides removed for subscription access.
/// The official client still owns its credential store and managed policy.
pub const CLAUDE_CODE_ENV_OVERRIDES: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_CUSTOM_HEADERS",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_SMALL_FAST_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_REFRESH_TOKEN",
    "CLAUDE_CODE_OAUTH_SCOPES",
    "CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR",
    "CLAUDE_CODE_API_KEY_FILE_DESCRIPTOR",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
    "CLAUDE_CODE_SIMPLE",
    "CLAUDECODE",
];

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{discover_program, resolve_claude_code_program};

    #[test]
    fn official_client_discovery_preserves_override_and_prefers_native_install()
    -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let home = root.path().join("home with space");
        let native_dir = home.join(".local/bin");
        let path_dir = root.path().join("path");
        std::fs::create_dir_all(&native_dir)?;
        std::fs::create_dir(&path_dir)?;
        let name = if cfg!(windows) {
            "claude.exe"
        } else {
            "claude"
        };
        let native = native_dir.join(name);
        let fallback = path_dir.join(name);
        for file in [&native, &fallback] {
            std::fs::write(file, [])?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o700))?;
            }
        }
        // A batch shim must not precede the native executable in automatic lookup.
        #[cfg(windows)]
        std::fs::write(path_dir.join("claude.cmd"), "@exit /b 1\r\n")?;
        let paths = std::env::join_paths([path_dir])?;
        let explicit = Path::new("unavailable/custom-client");
        assert_eq!(resolve_claude_code_program(Some(explicit)), explicit);
        assert_eq!(discover_program(Some(&home), Some(&paths)), native);
        std::fs::remove_file(&native)?;
        assert_eq!(discover_program(Some(&home), Some(&paths)), fallback);
        std::fs::remove_file(fallback)?;
        assert_eq!(discover_program(Some(&home), Some(&paths)), Path::new(name));
        Ok(())
    }
}
