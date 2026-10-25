# TD6-2026-10-25-01: 13 EDID+FULL-only record types are parsed into `EsmIndex` maps that nothing outside tests/examples ever reads

Labels: low, esm-plugin, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6387
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Dimension**: 6 · **Effort**: small (decide per type) · **Age**: #810/#458 (2026-05); table re-confirmed through 2026-10
- **Location**: dispatch `crates/plugin/src/esm/records/parse.rs:443-449` -> `dispatch_misc_stub.rs` (arms use `parse_minimal_esm_record`); maps in `records/index.rs`.
- **What**: Of the 31 minimal-tier record types, these `EsmIndex` fields have **zero non-test, non-parse readers** (`grep -w <field>` outside `tests/`, `examples/`, `index.rs`, `parse.rs`, `dispatch_misc_stub.rs`): `animation_objects` (ANIO), `camera_shots` (CAMS), `camera_paths` (CPTH), `default_objects` (DOBJ), `menu_icons` (MICN), `ammo_effects` (AMEF), `load_screen_types` (LSCT), `placeable_waters` (PWAT), `dehydration_stages`/`hunger_stages`/`sleep_deprivation_stages` (DEHY/HUNG/SLPD), `recipe_categories`/`recipe_records` (RCCT/RCPE), `birthsigns` (BSGN), `sigil_stones` (SGST). Per-record `HashMap<u32, MinimalEsmRecord>` + String allocs on every FNV/FO3/Oblivion load. The dispatch comment ("when a real consumer arrives, replace the arm") makes this a deliberate parking lot, but nothing in ROADMAP/docs lists which of these are genuinely owed: BSGN (Oblivion birthsign abilities never reach the player ruleset), HUNG/DEHY/SLPD (FNV Hardcore needs; `charal-fnv` has no consumer), RCPE/RCCT (crafting), CPTH/CAMS (the load-screen MOD2 camera paths in open #6194), DOBJ (engine default forms).
- **Fix**: (a) add one line per type to the ROADMAP "Known Issues -> Correctness and content" naming it as parsed-not-consumed with the owning future milestone, or (b) drop the maps for types with no owner (ANIO, MICN, AMEF, LSCT, PWAT, SGST) and let them fall to `skipped_unconsumed_groups`, which already gives the telemetry. Not a bug; the cost is parse time + memory for dead data.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD6-2026-10-25-01`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
