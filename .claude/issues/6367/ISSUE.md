# TD2-2026-10-25-08: three more Option-returning little-endian cursors beside `SubReader`

Labels: low, esm-plugin, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6367
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Dimension**: 2
- **Location**: `crates/plugin/src/esm/records/script_instance.rs:835-885` (`Cursor`: `take/u8/u16/i16/u32/i32/f32/wstring`), `crates/plugin/src/esm/records/weather_settings_instance.rs:143-175` (`Cursor`: `take/u16/i32/u32/at_end`, plus `bytes.try_into().unwrap()` decode in `read_builtin` `:443-466`), `crates/plugin/src/esm/records/misc/world.rs:427-460` (`NvnmCursor`). Canonical: `crates/plugin/src/esm/sub_reader.rs` (module doc: replaces "inline `from_le_bytes` patterns"). `reflection.rs:271-332` mixes `SubReader` and raw `from_le_bytes(blob[..].try_into().unwrap())` in one function (10 sites).
- **Effort**: small
- **Fix**: add `SubReader::take(n) -> Result<&[u8]>`, `is_at_end()`, `wstring()`/`counted(stride)`; the three cursors become `SubReader` with `.ok()?`. Net: three private types gone, one bounds policy. (Net `from_le_bytes` count in `esm/` fell 34→17 this window — the trend is right, this is the remaining structural copies.)

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD2-2026-10-25-08`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
