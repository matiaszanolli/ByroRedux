# PAR-D6-2026-09-21-02: Duplicate names inside one archive overwrite silently, and BSA keys are not separator-normalised at open

Labels: low,bug,import-pipeline

## Description
`crates/bsa/src/archive/open.rs:361-369` and `crates/bsa/src/ba2.rs:323-326`: both `HashMap::insert` call sites are last-wins with no log line on a collision.

Separately, `crates/bsa/src/archive/open.rs:252` only `.to_lowercase()`s BSA folder names at open time; BA2 keys go through `normalize_path` (`ba2.rs:309`) but BSA keys do not get the equivalent separator normalisation. A third-party BSA storing `/` in a folder name would produce keys that no normalised query can reach.

Verified unchanged at HEAD `ee6d3fb39`: both inserts are still unconditional `HashMap::insert` with no duplicate-count tracking, and BSA folder-name normalisation is still lowercase-only.

## Evidence
Probe `dup-scan`: 441 installed archives across all eight titles (vanilla plus installed mods) — 0 with declared count != distinct keys, i.e. hygiene-only on current content.

## Impact
No observed impact on current content; latent hygiene gap for a hand-crafted or unusually-authored third-party archive.

## Related
#3637 (the shadow-count logging precedent this should follow)

## Suggested Fix
Count overwritten keys at open and log once per archive, and apply `normalize_path` to BSA keys at open to match BA2.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D6-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix