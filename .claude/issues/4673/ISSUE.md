# PAR-D6-2026-09-21-04: game-detect has unbounded VDF recursion and joins ACF installdir without containment, and unreadable manifests vanish silently

Labels: low,bug,import-pipeline

## Description
`crates/game-detect/src/vdf.rs:107-159` (`Cursor::parse_entries`) recurses once per `{` block with no depth counter anywhere in the function (`grep -n depth crates/game-detect/src/vdf.rs` returns nothing).

`crates/game-detect/src/steam.rs:157-158`: `steamapps.join("common").join(install_dir)` — an absolute `installdir` value replaces the base path entirely rather than merely escaping it via `..`, and only `is_dir()` gates the result before it is reported as a detected install.

`crates/game-detect/src/steam.rs:82` and `:139`: `let Ok(text) = std::fs::read_to_string(..) else { continue }` drops an unreadable or non-UTF-8 manifest with no log line, while a genuine VDF *parse* error two lines later does `log::warn!`.

Verified unchanged at HEAD `ee6d3fb39`: no `depth` identifier exists anywhere in `vdf.rs`; both `steam.rs` read sites still silently `continue` on a read failure.

## Evidence
See the cited locations. The skill's own Dim 6 notes already record the recursion and the join as Steam-written and LOW severity.

## Impact
Requires a tampered Steam install to trigger. The launcher could report an install outside the intended library, or crash on a pathological VDF nesting depth.

## Related
`/audit-tooling` Dim 5 (policy owner for launcher/install-detect surfaces)

## Suggested Fix
Add a depth cap (for example 32) to `parse_entries`, and reject `installdir` values that are absolute or contain `..` components. Log a debug line on manifest read failures to match the parse-error path.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D6-2026-09-21-04)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix