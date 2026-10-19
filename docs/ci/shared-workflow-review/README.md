# Shared workflow review source

This directory supplies the external source needed to review the paired PR
and draft-release caller update. It contains documentation data, with `.txt`
suffixes and no executable modes. The workflow callers, product gate, runtime
and dependency pins do not read or execute these copies.

## Source revisions

- Repository: `ejmlabs/.github`.
- Previous caller: `5fae851502c2e3c31d4df9a44f8d56c2cb49e098`.
- Proposed caller: `ae903129564926a14287f75d2012610c59cccb65`.

The proposed implementation merged through protected central main as
`8c304c8cdfc8b6fe50f3e442507215e615944f2b`, with the same tree
`91b7f5c0990e27f2e0fdfcce26aac18afdcf6b81` as the reviewed head.
Its exact head is retained at `refs/heads/workflow-pins/ae903129564926a14287f75d2012610c59cccb65`.
That central receipt does not complete either consumer review.

## Contents and integrity

[The manifest](manifest.json) maps each source revision and original path to
its exact snapshot, Git blob SHA-1, SHA-256 digest, byte count and line count.
Preparation compared full-revision source bytes and recomputed their Git blob
IDs with Git. Matching blobs have one shared copy. The 34 source-path records
use 21 plaintext files containing 537572 bytes.
The digests identify content; they are not digital signatures or independent
proof of maintainer approval. [The copied MIT notice](LICENSE.txt) covers the
central source.

The generated workflow copies preserve the actual called implementation.
The generator, libraries and templates expose the readable source embedded as
base64 in those workflows. Both release and PR behavior belong to the pin
comparison. Four original paths differ between the two revisions: the two
generated workflows, `scripts/build_workflows.py` and `scripts/standards.py`.

| Original central path                        | Previous source                                               | Proposed source                                               |
| -------------------------------------------- | ------------------------------------------------------------- | ------------------------------------------------------------- |
| `.github/workflows/governance.yml`           | [text](old/workflows/governance.yml.txt)                      | [text](new/workflows/governance.yml.txt)                      |
| `.github/workflows/draft-release.yml`        | [text](old/workflows/draft-release.yml.txt)                   | [text](new/workflows/draft-release.yml.txt)                   |
| `scripts/build_workflows.py`                 | [text](old/scripts/build_workflows.py.txt)                    | [text](new/scripts/build_workflows.py.txt)                    |
| `scripts/governance.py`                      | [text](common/scripts/governance.py.txt)                      | [text](common/scripts/governance.py.txt)                      |
| `scripts/prepare.py`                         | [text](common/scripts/prepare.py.txt)                         | [text](common/scripts/prepare.py.txt)                         |
| `scripts/release.py`                         | [text](common/scripts/release.py.txt)                         | [text](common/scripts/release.py.txt)                         |
| `scripts/review_result.py`                   | [text](common/scripts/review_result.py.txt)                   | [text](common/scripts/review_result.py.txt)                   |
| `scripts/standards.py`                       | [text](old/scripts/standards.py.txt)                          | [text](new/scripts/standards.py.txt)                          |
| `review/prompt.md`                           | [text](common/review/prompt.md.txt)                           | [text](common/review/prompt.md.txt)                           |
| `review/schema.json`                         | [text](common/review/schema.json.txt)                         | [text](common/review/schema.json.txt)                         |
| `LICENSE`                                    | [text](LICENSE.txt)                                           | [text](LICENSE.txt)                                           |
| `templates/community/CODE_OF_CONDUCT.md`     | [text](common/templates/community/CODE_OF_CONDUCT.md.txt)     | [text](common/templates/community/CODE_OF_CONDUCT.md.txt)     |
| `templates/community/CONTRIBUTING.md`        | [text](common/templates/community/CONTRIBUTING.md.txt)        | [text](common/templates/community/CONTRIBUTING.md.txt)        |
| `templates/community/RELEASING.md`           | [text](common/templates/community/RELEASING.md.txt)           | [text](common/templates/community/RELEASING.md.txt)           |
| `templates/community/SECURITY.md`            | [text](common/templates/community/SECURITY.md.txt)            | [text](common/templates/community/SECURITY.md.txt)            |
| `templates/community/SUPPORT.md`             | [text](common/templates/community/SUPPORT.md.txt)             | [text](common/templates/community/SUPPORT.md.txt)             |
| `templates/community/THIRD_PARTY_NOTICES.md` | [text](common/templates/community/THIRD_PARTY_NOTICES.md.txt) | [text](common/templates/community/THIRD_PARTY_NOTICES.md.txt) |

## Review transport and limits

The existing trusted preparation job archives committed consumer files into
`product.tar`. Each independent reviewer receives that archive under
`review-data/product/`, along with `context.json` and `change.patch`. It does
not receive a central checkout. PR bodies, comments and remote URLs are not
source evidence in that archive. The trusted reviewer uses text-only,
read-only inspection without network access; a binary Git bundle alone would
not supply readable implementation context.

These copies remain untrusted review material. They change no trusted prompt,
review authority, permissions, action policy, human admission or exact-head
approval. They are source context, not proof that a review completed. A new
consumer head still needs its normal gate, fresh admission, two complete
reviews and its own explicit approval before protected adoption.

The earlier caller-head review reports explicitly said the external-source
comparison was incomplete despite successful wrapper jobs and zero findings.
Those reports do not establish acceptance. Source publication, hosted quality
and completed review remain separate checks.
