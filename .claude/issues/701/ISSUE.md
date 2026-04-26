# Issue #701: O2-3: dead-code warning on FolderRecord.{hash, offset} in release builds

**Severity**: LOW (cosmetic)
**File**: `crates/bsa/src/archive.rs:215`
**Dimension**: BSA v103 Archive

`cargo run --release` surfaces:
```
warning: fields `hash` and `offset` are never read
   --> crates/bsa/src/archive.rs:215:13
```

Both fields are read inside `#[cfg(debug_assertions)]` blocks (at archive.rs:299 for `hash` and archive.rs:271 for `offset`) for the #361 + #362 hash/offset validators, but the compiler doesn't see those reads in release builds and warns. Cosmetic only — does not affect parse.

**Fix**: Gate the field declarations themselves with `#[cfg(debug_assertions)]`, or add `#[allow(dead_code)]` on the struct (mirrors the pattern at archive.rs:22 / 60 for `genhash_folder` / `genhash_file`).

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
