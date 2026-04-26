# Issue #690: O2-1: zero on-disk v103 BSA regression test coverage

**Severity**: MEDIUM
**File**: `crates/bsa/src/archive.rs:533-1387`
**Dimension**: BSA v103 Archive

The 14 `#[ignore]`-gated integration tests against real Steam BSAs all target either FNV v104 (`FNV_MESHES_BSA = Fallout - Meshes.bsa`) or Skyrim SE v105 (`SKYRIM_MESHES0_BSA / SKYRIM_MESHES1_BSA / SKYRIM_TEXTURES0_BSA`). **Zero target v103.**

Empirical extraction sweep is at 100% (147,629 / 147,629 across all 17 vanilla Oblivion BSAs in 2026-04-25 audit), but a regression in the v103-specific code paths — the `version >= 104` gate at `archive.rs:188` (embed_file_names denial), the v103 = 16-byte folder record sizing at `archive.rs:213`, the v103 → zlib branch at `archive.rs:499` — would not show up in `cargo test -- --ignored` even on a fully-installed dev box.

The #569 (Skyrim v105 disk tests) and #617 (synthetic v105) work has filled the v105 coverage gap but left v103 untouched.

**Fix sketch**: Add a sibling `#[ignore]`'d disk test against `Oblivion - Meshes.bsa` pinning file_count = 20,182 and round-tripping a known small mesh. Mirror the FNV `extract_beer_bottle` pattern. ~30 lines.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
