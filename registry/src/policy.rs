use ejm_harness_package_spec::Manifest;
use serde::{Deserialize, Serialize};

pub const MAX_METADATA_BYTES: usize = ejm_harness_package_spec::MAX_MANIFEST_BYTES + 1024;
pub const MAX_ARCHIVE_BYTES: usize = ejm_harness_package_spec::MAX_ARCHIVE_BYTES;
pub const MAX_AUTH_BYTES: usize = 16 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Publication {
    pub manifest: Manifest,
    pub sha256: String,
    pub size: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublisherInput {
    pub handle: String,
    pub display_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModerationInput {
    pub disabled: bool,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct Identity {
    pub id: String,
    pub display_name: String,
    pub is_operator: bool,
}

#[must_use]
pub fn valid_handle(value: &str) -> bool {
    (3..=39).contains(&value.len())
        && valid_component(value)
        && ![
            "admin",
            "api",
            "ejm",
            "ejmlabs",
            "harness",
            "help",
            "moderator",
            "root",
            "security",
            "support",
            "system",
            "www",
        ]
        .contains(&value)
}

#[must_use]
pub fn valid_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 48
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-')
}

#[must_use]
pub fn valid_version(value: &str) -> bool {
    value.len() <= 128
        && semver::Version::parse(value)
            .is_ok_and(|version| version.to_string() == value && version.build.is_empty())
}

#[must_use]
pub fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Cookie authentication is accepted only for mutations from our own origin.
/// An explicit bearer must stand on its own and never falls back to a cookie.
#[must_use]
pub fn mutation_credentials_allowed(
    authorization: Option<&str>,
    cookie: Option<&str>,
    origin: Option<&str>,
    expected_origin: &str,
) -> bool {
    if let Some(authorization) = authorization {
        return authorization.strip_prefix("Bearer ").is_some_and(|token| {
            !token.is_empty()
                && token.len() <= 4096
                && token.bytes().all(|byte| byte.is_ascii_graphic())
        }) && origin.is_none_or(|origin| origin == expected_origin);
    }
    cookie.is_some_and(|cookie| !cookie.is_empty() && cookie.len() <= 8192)
        && origin == Some(expected_origin)
}

#[must_use]
pub fn manifest_identity(manifest: &Manifest) -> (&str, &str, &str) {
    (&manifest.name, &manifest.version, &manifest.description)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_auth_rejects_csrf_and_invalid_bearer_cookie_fallback() {
        let origin = "https://registry.example.invalid";
        assert!(!mutation_credentials_allowed(
            None,
            Some("session=secret"),
            None,
            origin
        ));
        assert!(!mutation_credentials_allowed(
            None,
            Some("session=secret"),
            Some("https://evil.example"),
            origin
        ));
        assert!(!mutation_credentials_allowed(
            Some("Basic forged"),
            Some("session=secret"),
            Some(origin),
            origin
        ));
        assert!(!mutation_credentials_allowed(
            Some("Bearer "),
            None,
            None,
            origin
        ));
        assert!(!mutation_credentials_allowed(
            Some("Bearer bad token"),
            None,
            None,
            origin
        ));
        assert!(!mutation_credentials_allowed(
            Some("Bearer valid"),
            None,
            Some("null"),
            origin
        ));
        assert!(mutation_credentials_allowed(
            None,
            Some("session=secret"),
            Some(origin),
            origin
        ));
        assert!(mutation_credentials_allowed(
            Some("Bearer token"),
            None,
            None,
            origin
        ));
    }

    #[test]
    fn identifiers_cannot_escape_keys_or_claim_reserved_names() {
        for handle in [
            "ejm",
            "ejmlabs",
            "admin",
            "a",
            "Evil",
            "../alice",
            "a/b",
            "-alice",
            "alice-",
            "álîce",
            "alice%2fbob",
        ] {
            assert!(!valid_handle(handle), "{handle}");
        }
        assert!(valid_handle("alice-42"));
        assert!(valid_version("1.2.3-beta.1"));
        for version in ["latest", "v1.2.3", "01.2.3", "../1.2.3", "1.2.3+build"] {
            assert!(!valid_version(version));
        }
        assert!(valid_digest(&"a".repeat(64)));
        assert!(!valid_digest(&"A".repeat(64)));
        assert!(!valid_digest(&"g".repeat(64)));
    }
}
