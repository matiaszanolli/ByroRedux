# #4384 — TD8-005: `crates/plugin/src/legacy/mod.rs` hides a 5.5-month-old consumer-less module behind `#![allow(dead_code)]`, while the live ESM path models slots separately

**Labels**: low, esm-plugin, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4384

- **Severity**: LOW · **Dimension**: 8
- **Location**: `crates/plugin/src/legacy/mod.rs:35`, `crates/plugin/src/lib.rs:35`; parallel model `crates/plugin/src/esm/reader.rs:408-435` (`GlobalSlot::compose`, `FormIdRemap`) · **Status**: NEW (#1322 kept it as scaffolding; the evidence below is new) · **Age**: `bed67b87b` (2026-03-28) · **Effort**: small · **Kind**: tech-debt
- **Finding**: `LegacyFormId` / `LegacyLoadOrder` have no production caller, and neither does the `DataStore::add_plugin` layer they would feed. There are two independent ESL `0xFE` models, and only the dead one knows ESH `0xFD`. #4081 (2026-09-11) spent a fix on `LegacyFormId::is_null` in code nothing executes. The module-wide allow also hides any future dead additions.
- **Suggested Fix**: Delete the module and fold ESH `0xFD` into `GlobalSlot` when needed; or keep it, add a tracker for the esm→`Record` bridge, and plan to reuse `GlobalSlot::compose`.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
