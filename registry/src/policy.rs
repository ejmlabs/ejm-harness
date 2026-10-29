use ejm_harness_package_spec::Manifest;
use serde::Deserialize;

pub const MAX_METADATA_BYTES: usize = ejm_harness_package_spec::MAX_MANIFEST_BYTES + 1024;
pub const MAX_ARCHIVE_BYTES: usize = ejm_harness_package_spec::MAX_ARCHIVE_BYTES;
pub const MAX_AUTH_BYTES: usize = 16 * 1024;
pub const MAX_HEADER_BYTES: usize = 8192;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Publication {
    pub manifest: Manifest,
    pub sha256: String,
    pub size: u32,
}

impl Publication {
    /// A well-formed reservation for exactly `@publisher/name`.
    #[must_use]
    pub fn valid_for(&self, publisher: &str, name: &str) -> bool {
        self.manifest.validate().is_ok()
            && valid_version(&self.manifest.version)
            && self.manifest.name == format!("@{publisher}/{name}")
            && valid_digest(&self.sha256)
            && self.size != 0
            && u64::from(self.size) <= MAX_ARCHIVE_BYTES as u64
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublisherInput {
    pub handle: String,
    pub display_name: String,
}

impl PublisherInput {
    #[must_use]
    pub fn valid(&self) -> bool {
        valid_handle(&self.handle) && valid_text(&self.display_name, 80)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModerationInput {
    pub disabled: bool,
    pub reason: String,
}

impl ModerationInput {
    #[must_use]
    pub fn valid(&self) -> bool {
        valid_text(&self.reason, 512)
    }
}

#[derive(Debug, Deserialize)]
pub struct Identity {
    pub id: String,
    pub display_name: String,
    pub is_operator: bool,
}

impl Identity {
    /// Bounds on what the private auth service may return.
    #[must_use]
    pub fn valid(&self) -> bool {
        !self.id.is_empty() && self.id.len() <= 128 && self.display_name.len() <= 512
    }
}

fn valid_text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
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
    cookie.is_some_and(|cookie| !cookie.is_empty() && cookie.len() <= MAX_HEADER_BYTES)
        && origin == Some(expected_origin)
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

    fn publication(name: &str, version: &str, sha256: &str, size: u32) -> Option<Publication> {
        serde_json::from_value(serde_json::json!({
            "manifest": {
                "schema_version": 1,
                "name": name,
                "version": version,
                "description": "Fixture",
            },
            "sha256": sha256,
            "size": size,
        }))
        .ok()
    }

    #[test]
    fn reservations_match_their_route_and_stay_within_archive_bounds() {
        let digest = "a".repeat(64);
        let maximum = u32::try_from(MAX_ARCHIVE_BYTES).unwrap_or(u32::MAX);
        let valid = |name, version, sha256: &str, size| {
            publication(name, version, sha256, size)
                .is_some_and(|publication| publication.valid_for("alice", "test"))
        };
        assert!(valid("@alice/test", "1.0.0", &digest, 1));
        assert!(valid("@alice/test", "1.0.0", &digest, maximum));
        assert!(!valid("@alice/test", "1.0.0", &digest, 0));
        assert!(!valid("@alice/test", "1.0.0", &digest, maximum + 1));
        assert!(!valid("@bob/test", "1.0.0", &digest, 1));
        assert!(!valid("@alice/other", "1.0.0", &digest, 1));
        assert!(!valid("@alice/test", "1.0.0+build", &digest, 1));
        assert!(!valid("@alice/test", "1.0.0", &"A".repeat(64), 1));
    }

    #[test]
    fn text_fields_and_identities_are_bounded() {
        let publisher = |handle: &str, display_name: &str| {
            PublisherInput {
                handle: handle.into(),
                display_name: display_name.into(),
            }
            .valid()
        };
        assert!(publisher("alice", "Alice"));
        assert!(publisher("alice", &"x".repeat(80)));
        assert!(!publisher("alice", &"x".repeat(81)));
        assert!(!publisher("alice", " "));
        assert!(!publisher("alice", "Al\nice"));
        assert!(!publisher("admin", "Alice"));
        let reason = |reason: &str| {
            ModerationInput {
                disabled: true,
                reason: reason.into(),
            }
            .valid()
        };
        assert!(reason(&"x".repeat(512)));
        assert!(!reason(&"x".repeat(513)));
        assert!(!reason(""));
        assert!(!reason("bad\u{7f}"));
        let identity = |id: &str, display_name: &str| {
            Identity {
                id: id.into(),
                display_name: display_name.into(),
                is_operator: false,
            }
            .valid()
        };
        assert!(identity(&"x".repeat(128), &"x".repeat(512)));
        assert!(!identity("", ""));
        assert!(!identity(&"x".repeat(129), ""));
        assert!(!identity("x", &"x".repeat(513)));
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
