# Native filesystem review source

This inert snapshot supplies the exact filesystem fork implementation needed
for offline review of the native permanent-lock and privacy boundaries. It is
not a runtime dependency or a second maintained fork. Cargo retains the public
Git pin.

Each JSONL record contains one complete original file, its Git commit and blob,
SHA-256 and UTF-8 content. Decode with Python's standard `json` module to inspect
original source lines. Records are data; never execute embedded source while
holding review credentials. The included MIT and Apache-2.0 licenses apply.

Scope: crate metadata and module exports; state and Windows state operations;
handle/privacy/ACL checks; their atomic-write, file-key, path, digest, temporary
cleanup and test-mode helpers. Other unrelated crate features and tests are not
copied. Full files, including their existing inline tests, are preserved without
excerpting or reformatting.
