# #3864: TD2-2026-09-05-07: 111 hand-written full-field `NifHeader` literals across 40 files, with ~18 rival local factory functions, while `NifHeader::detached` produces exactly that value

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD2-2026-09-05-07) via `/audit-publish`, 2026-09-05. Labels: `low,nif-parser,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3864 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD2-2026-09-05-07), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 2 — Logic Duplication
- **Location**: `crates/nif/src/header.rs` — `NifHeader::detached` (the
  consolidation site, 1 production caller);
  40 files under `crates/nif/src/` construct the literal by hand, among them
  `crates/nif/src/blocks/base.rs`, `crates/nif/src/blocks/skin.rs`,
  `crates/nif/src/blocks/texture.rs`, `crates/nif/src/blocks/multibound.rs`,
  `crates/nif/src/blocks/dispatch_tests/`, `crates/nif/src/blocks/shader_tests/`,
  `crates/nif/src/blocks/collision/`, `crates/nif/src/blocks/controller/`,
  `crates/nif/src/stream.rs`, `crates/nif/src/version.rs`
- **Status**: NEW
- **Description**: `NifHeader::detached(version, user_version, user_version_2)`
  builds exactly the twelve-field "minimal version context, every table empty"
  header that NIF block tests need — it was added for the M49 detached-CSG
  decode and has one production caller
  (`crates/nif/src/import/precombine.rs`). Every test fixture in the crate
  reimplements it: `grep -c "num_groups: 0,"` returns **111** across 40 files,
  and at least eighteen local factory functions wrap it under six different
  names (`header_at`, `make_header`, `test_header`, `make_header_fnv`,
  `make_header_fo4`, `make_header_oblivion`, `make_header_fo76`,
  `make_header_pre_oblivion_v10_2`, …), several of which are byte-identical
  across files (`base.rs` declares `header_at` twice in one file, and
  `dispatch_tests/legacy_particle.rs` + `dispatch_tests/controllers.rs` each
  declare a third and fourth identical copy).
- **Evidence**: the 14-line window scan over `crates/nif/src/blocks/` returns
  the `NifHeader { version, little_endian: true, user_version: 0, …,
  num_groups: 0 }` block as the single most-repeated fragment in the directory,
  6 occurrences across 5 files for one exact variant alone.
  `NifHeader::detached`'s body is that literal, field for field.
- **Impact**: Adding a thirteenth field to `NifHeader` breaks 111 sites instead
  of 1. (It is at least a compile error rather than silent — which is why this
  is LOW, not MEDIUM.) The secondary cost is that the six factory names make
  cross-file test reading harder than it needs to be.
- **Related**: #834 (the `Arc<str>` block-types change that already had to
  touch this many sites once).
- **Suggested Fix**: Migrate the fixtures to
  `NifHeader::detached(version, user_version, user_version_2)`, using
  `NifHeader { strings, max_string_length, ..NifHeader::detached(v, uv, uv2) }`
  for the handful that populate a string table. Then delete the per-file
  factories in favour of one `#[cfg(test)] pub(crate)` helper in
  `crates/nif/src/header.rs` for the recurring game presets (FNV / FO4 / FO76 /
  Oblivion). Mechanical and scriptable.
- **Effort**: small (≤2 h)

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
