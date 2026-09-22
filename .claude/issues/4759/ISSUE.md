# TOOL-D5-2026-09-22-01: game-detect validate() never structurally checks the main ESM plugin, unlike the archive check

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4759

## Description
`crates/game-detect/src/validate.rs`'s archive-validation loop (`:165-206`, `archive_opens` at `:100-114`) explicitly reasons that "a truncated download or a mod manager that mangled the file... the engine would fail mid-load rather than at startup" and calls the real archive header parser (`BsaArchive::open`/`Ba2Archive::open`) accordingly, reporting `Severity::Fail` on a parse error. The main-plugin check (`:143-163`) applies none of that reasoning: confirmed by reading the full branch — `esm.is_file()` plus a byte-count-as-MB informational `Severity::Ok` line (`:157-162`), with no attempt to open or structurally validate the ESM. Even the crate's own test fixtures never exercise a real header here — every fixture writes the literal 4 bytes `b"TES4"` with no further structure (not independently re-verified byte-for-byte in this pass, but the validate.rs code path itself confirms no parse call exists to exercise). `crates/plugin/src/esm/reader.rs` has a real TES4/record header parser available (`read_record_header`, per-game `EsmVariant`/`record_header_size`), and `game-detect`'s `Cargo.toml` has no `byroredux-plugin` dependency today — this is a wiring gap, not a missing capability.

## Evidence
```rust
// crates/game-detect/src/validate.rs:157-162 — size only, never opened
let size_mb = esm.metadata().map(|m| m.len()).unwrap_or(0) / (1024 * 1024);
checks.push(Check::new(Severity::Ok, "Main plugin", format!("{} ({size_mb} MB)", entry.esm)));
```
```rust
// crates/game-detect/src/validate.rs:100-114 (archive branch, contrast)
fn archive_opens(path: &Path) -> Result<(), String> {
    ...
    "ba2" => Ba2Archive::open(path).map(|_| ()).map_err(|e| e.to_string()),
    _ => BsaArchive::open(path).map(|_| ()).map_err(|e| e.to_string()),
```

## Impact
`byro-detect`/the launcher pre-flight reports a broken install as "ready" when the corruption is in the ESM rather than an archive, defeating the tool's stated purpose.

## Related
None found. Distinct from #4673 (open — game-detect VDF recursion / ACF installdir containment), which is about Steam-manifest parsing, not main-plugin structural validation.

## Suggested Fix
Call the existing ESM header parser (or at minimum check the leading `TES4`/`TES3` magic and a sane minimum size) in the main-plugin branch, mirroring the archive branch's `Severity::Fail` on parse error.

Source: docs/audits/AUDIT_TOOLING_2026-09-22.md (TOOL-D5-2026-09-22-01)

## Completeness Checks
- [ ] **TESTS**: A test fixture with a truncated/corrupt (but existent, correctly-sized-on-disk) ESM asserts `validate()` reports `Severity::Fail`, mirroring the existing archive-corruption test
