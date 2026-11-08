# ESM-D4-04: Doc rot after the window — `categories()` is 117 and the WEAP `ETYP` comment predates #6657

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6792
Labels: low, esm-plugin, documentation, doc-rot
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW
- **Dimension**: 4 · **Status**: NEW
- **Location**:
  - `docs/engine/esm-records.md:54` says "115 (pinned by the `categories()` length assertion)"; `records/tests.rs:2020` pins **117**.
  - `.claude/commands/audit-esm/SKILL.md:337` says "115 at HEAD".
  - `crates/plugin/src/esm/records/items.rs` ~`:591`: "embedded equip-type FormID (an EQUP record)".
- **Evidence**:
  - `3bfcbd58b` bumped 115 -> 117 for `volumetric_lighting` + `god_rays` and touched neither doc.
  - The ETYP comment is stale: after `a290c755f` the field is `equip_type`, "an enum on FO3/FNV (xEdit `wbEquipTypeEnum`, FalloutNV.esm holds 0-7), not a form at all". The arm still pushes it through `remap_fid` for every game.
  - `FormIdRemap::remap` maps raw 0-7 through `master_slots[0]` / the self slot; that is identity for every shipped master and DLC, so no wrong value is produced on shipped data.
- **Impact**: documentation only (plus an unconditional `remap_fid` of a non-FormID on FO3/FNV, benign on shipped data).
- **Suggested fix**: update both docs, or say "see `records/tests.rs`" instead of a number. Gate the `ETYP` remap to the games where it is an `EQUP` form, or reword the comment.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
