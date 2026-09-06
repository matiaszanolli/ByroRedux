# #3865: TD2-2026-09-05-08: the `SubRecord` test-fixture builder is defined 32 times across `crates/plugin`, under three names and three incompatible signatures

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD2-2026-09-05-08) via `/audit-publish`, 2026-09-05. Labels: `low,esm-plugin,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3865 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD2-2026-09-05-08), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 2 — Logic Duplication
- **Location**: 29 files under `crates/plugin/src/` each declare their own; e.g.
  `crates/plugin/src/esm/records/common.rs`,
  `crates/plugin/src/esm/records/items.rs`,
  `crates/plugin/src/esm/records/actor/tests.rs`,
  `crates/plugin/src/esm/records/misc/world.rs` (three times in one file),
  `crates/plugin/src/esm/records/misc/quest.rs`,
  `crates/plugin/src/esm/records/misc/pack.rs`,
  `crates/plugin/src/esm/records/misc/water.rs`,
  `crates/plugin/src/esm/records/scol.rs` (twice),
  `crates/plugin/src/esm/records/movs.rs`, `.../outfit.rs`, `.../pkin.rs`,
  `.../soun.rs`, `.../list_record.rs`, `.../climate.rs`, `.../weather.rs`,
  `crates/plugin/src/esm/cell/support.rs`
- **Status**: NEW
- **Description**: Every ESM record test module opens by redeclaring the same
  two-line constructor. Three names are in circulation (`sub`, `mk_sub`,
  `make_sub`) across three signatures — `(&[u8; 4], &[u8])`,
  `(&[u8; 4], Vec<u8>)`, `([u8; 4], Vec<u8>)`, plus one
  `(&[u8; 4], impl Into<Vec<u8>>)` in `misc/scene.rs` — so a test moved between
  files does not compile. Six files additionally redeclare identical `edid()`
  and `modl()` zstring wrappers (`movs.rs`, `scol.rs`, `pkin.rs`, `outfit.rs`,
  `soun.rs`, `list_record.rs`), which the window scan reports as the single most
  duplicated fragment in `crates/plugin/src/esm/records/`.
- **Evidence**: 32 definitions matching
  `fn (mk_)?sub|fn make_sub` returning `SubRecord` across 29 files;
  116 `SubRecord { … }` literals in the workspace.
- **Impact**: Pure friction rather than risk — but it is the reason a new record
  parser's test module starts with boilerplate instead of a test, and the
  three-signature split actively discourages moving coverage between files.
  Reported here rather than under Dim 9 per the cross-dimension rule: the defect
  is duplication, not test quality.
- **Related**: #1631 (TD7-002, CLOSED — the CNTO-size duplication in the same
  tree); #2414 / #2068 (the production-side `CommonNamedFields` consolidations
  that already landed).
- **Suggested Fix**: Add `crates/plugin/src/esm/records/test_support.rs`, gated
  `#[cfg(test)]` and declared `pub(crate)`, holding one
  `sub(typ: &[u8; 4], data: impl Into<Vec<u8>>) -> SubRecord` (the widest of the
  existing signatures, so every current call site compiles unchanged) plus
  `edid(&str)` / `modl(&str)` / `full(&str)`. Delete the 32 local copies. The
  `#[cfg(test)] pub(crate)` shape is already used elsewhere in this workspace
  (`crates/nif/src/blocks/controller/tests.rs` exposes `pub(super)
  make_header_fnv`), so the pattern is established.
- **Effort**: small (≤2 h)

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
