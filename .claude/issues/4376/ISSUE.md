# #4376 — TD6-009: SPEL / ENCH / MGEF / LVSP maps (plus the Oblivion effect-code index) are parsed on every ESM load with zero production readers

**Labels**: low, esm-plugin, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4376

- **Severity**: LOW · **Dimension**: 6
- **Location**: `crates/plugin/src/esm/records/index.rs:137,211,217,219,231`; inserts in `dispatch_misc_gameplay_b.rs` / `dispatch_container.rs` · **Status**: NEW (no open issue mentions magic/SPEL/MGEF/SPLO) · **Effort**: trivial (tracker) / large (magic runtime) · **Kind**: tech-debt
- **Finding**: MQ101's `AddRaceSpells` already declines for want of a SPLO decoder. Dim 6's report also tabulates ~45 other zero-reader `EsmIndex` fields (ECZN, NAVI, FLST, CSTY, PROJ/EXPL/IPCT, MESG, REPU, BPTD, COBJ, plus the deliberate EDID+FULL long tail); all 77 `ImportedMaterial` fields have readers.
- **Suggested Fix**: Open a "magic runtime: SPLO → spell lists → MGEF application" tracker and reference it from the `leveled_spells` / `magic_effects_by_code` docs. Keep the parsers.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
