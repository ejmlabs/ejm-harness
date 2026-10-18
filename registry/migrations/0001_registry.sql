PRAGMA foreign_keys = ON;

CREATE TABLE registry_limits (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  reserved_bytes INTEGER NOT NULL CHECK (reserved_bytes > 0),
  versions INTEGER NOT NULL CHECK (versions > 0),
  accounts INTEGER NOT NULL CHECK (accounts > 0),
  publishers INTEGER NOT NULL CHECK (publishers > 0),
  daily_upload_bytes INTEGER NOT NULL CHECK (daily_upload_bytes > 0),
  daily_upload_count INTEGER NOT NULL CHECK (daily_upload_count > 0)
);
INSERT INTO registry_limits VALUES (1,1073741824,20000,20000,5000,268435456,2000);

CREATE TRIGGER registry_limits_cannot_be_deleted
BEFORE DELETE ON registry_limits
BEGIN
  SELECT RAISE(ABORT, 'registry limits are required');
END;

CREATE TABLE registry_accounts (
  id TEXT PRIMARY KEY NOT NULL,
  suspended INTEGER NOT NULL DEFAULT 0 CHECK (suspended IN (0, 1)),
  suspension_reason TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE publishers (
  handle TEXT PRIMARY KEY NOT NULL,
  user_id TEXT NOT NULL UNIQUE REFERENCES registry_accounts(id),
  display_name TEXT NOT NULL,
  suspended INTEGER NOT NULL DEFAULT 0 CHECK (suspended IN (0, 1)),
  suspension_reason TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE versions (
  publisher TEXT NOT NULL REFERENCES publishers(handle),
  name TEXT NOT NULL,
  version TEXT NOT NULL,
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  description TEXT NOT NULL,
  sha256 TEXT NOT NULL CHECK (length(sha256) = 64),
  size INTEGER NOT NULL CHECK (size > 0 AND size <= 8388608),
  ready INTEGER NOT NULL DEFAULT 0 CHECK (ready IN (0, 1)),
  quarantined INTEGER NOT NULL DEFAULT 0 CHECK (quarantined IN (0, 1)),
  quarantine_reason TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL DEFAULT (unixepoch()),
  published_at INTEGER,
  PRIMARY KEY (publisher, name, version)
);

CREATE INDEX versions_catalog ON versions(ready, quarantined, publisher, name);
CREATE INDEX versions_publisher ON versions(publisher, name, published_at);

CREATE TABLE rate_limits (
  key TEXT PRIMARY KEY NOT NULL,
  count INTEGER NOT NULL,
  bytes INTEGER NOT NULL,
  expires_at INTEGER NOT NULL
);
CREATE INDEX rate_limits_expiry ON rate_limits(expires_at);

CREATE TABLE moderation_log (
  id INTEGER PRIMARY KEY,
  operator_id TEXT NOT NULL,
  target_type TEXT NOT NULL,
  target TEXT NOT NULL,
  disabled INTEGER NOT NULL CHECK (disabled IN (0, 1)),
  reason TEXT NOT NULL,
  created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TRIGGER publisher_identity_is_immutable
BEFORE UPDATE OF handle, user_id ON publishers
BEGIN
  SELECT RAISE(ABORT, 'publisher identity is immutable');
END;

CREATE TRIGGER version_identity_is_immutable
BEFORE UPDATE OF publisher, name, version, manifest_json, description, sha256, size ON versions
BEGIN
  SELECT RAISE(ABORT, 'version identity is immutable');
END;

CREATE TRIGGER version_cannot_be_deleted
BEFORE DELETE ON versions
BEGIN
  SELECT RAISE(ABORT, 'version identity is permanent');
END;

CREATE TRIGGER version_cannot_be_unpublished
BEFORE UPDATE OF ready ON versions
WHEN OLD.ready = 1 AND NEW.ready = 0
BEGIN
  SELECT RAISE(ABORT, 'published version is immutable');
END;

CREATE TRIGGER publisher_requires_active_account
BEFORE INSERT ON publishers
WHEN NOT EXISTS (SELECT 1 FROM registry_accounts WHERE id = NEW.user_id AND suspended = 0)
BEGIN
  SELECT RAISE(ABORT, 'account unavailable');
END;

CREATE TRIGGER registry_account_quota
BEFORE INSERT ON registry_accounts
WHEN NOT EXISTS (SELECT 1 FROM registry_accounts WHERE id = NEW.id)
  AND (SELECT count(*) FROM registry_accounts) >= (SELECT accounts FROM registry_limits WHERE id = 1)
BEGIN
  SELECT RAISE(ABORT, 'registry account quota reached');
END;

CREATE TRIGGER registry_publisher_quota
BEFORE INSERT ON publishers
WHEN NOT EXISTS (SELECT 1 FROM publishers WHERE handle = NEW.handle OR user_id = NEW.user_id)
  AND (SELECT count(*) FROM publishers) >= (SELECT publishers FROM registry_limits WHERE id = 1)
BEGIN
  SELECT RAISE(ABORT, 'registry publisher quota reached');
END;

CREATE TRIGGER publication_requires_active_owner
BEFORE INSERT ON versions
WHEN NOT EXISTS (
  SELECT 1 FROM publishers p JOIN registry_accounts a ON a.id = p.user_id
  WHERE p.handle = NEW.publisher AND p.suspended = 0 AND a.suspended = 0
)
BEGIN
  SELECT RAISE(ABORT, 'publisher unavailable');
END;

CREATE TRIGGER publication_quota
BEFORE INSERT ON versions
WHEN (SELECT count(*) FROM versions WHERE publisher = NEW.publisher) >= 1000
  OR (SELECT coalesce(sum(size), 0) FROM versions WHERE publisher = NEW.publisher) + NEW.size > 268435456
  OR ((SELECT count(DISTINCT name) FROM versions WHERE publisher = NEW.publisher) >= 100
    AND NOT EXISTS (SELECT 1 FROM versions WHERE publisher = NEW.publisher AND name = NEW.name))
BEGIN
  SELECT RAISE(ABORT, 'publisher quota reached');
END;

CREATE TRIGGER registry_publication_quota
BEFORE INSERT ON versions
WHEN (SELECT count(*) FROM versions) >= (SELECT versions FROM registry_limits WHERE id = 1)
  OR (SELECT coalesce(sum(size),0) FROM versions) + NEW.size > (SELECT reserved_bytes FROM registry_limits WHERE id = 1)
BEGIN
  SELECT RAISE(ABORT, 'registry storage quota reached');
END;

CREATE TRIGGER moderation_requires_active_account
BEFORE INSERT ON moderation_log
WHEN NOT EXISTS (SELECT 1 FROM registry_accounts WHERE id = NEW.operator_id AND suspended = 0)
BEGIN
  SELECT RAISE(ABORT, 'operator account unavailable');
END;

CREATE TRIGGER version_ready_requires_active_owner
BEFORE UPDATE OF ready ON versions
WHEN NEW.ready = 1 AND (NEW.quarantined = 1 OR NOT EXISTS (
  SELECT 1 FROM publishers p JOIN registry_accounts a ON a.id = p.user_id
  WHERE p.handle = NEW.publisher AND p.suspended = 0 AND a.suspended = 0
))
BEGIN
  SELECT RAISE(ABORT, 'publisher unavailable');
END;
