# ESM-2026-10-31-04: Two doc blocks displaced onto the wrong item by inserted code

Labels: low,esm-plugin,documentation,doc-rot

Source: `docs/audits/AUDIT_ESM_2026-10-31.md` (ESM-2026-10-31-04). Severity LOW (doc-rot). Same class as #6454 and #6410.

**Location**: `crates/plugin/src/esm/records/misc/pack.rs:336-342`; `byroredux/src/cell_loader/load_order.rs:1957-1960`

## Description
- (a) `PKDT_INTERRUPT_RANDOM_CONVERSATIONS` was inserted between the `PROCEDURE_SANDBOX` doc block and its const. The Sandbox paragraph ("56 % of vanilla FNV NPCs carry one...") now sits on the wrong constant; `PROCEDURE_SANDBOX` itself has no doc.
- (b) The new test `use_land_data_children_inherit_on_oblivion_fo3_fnv_and_fo4_only` was inserted above `the_cache_and_the_processing_step_share_one_gate`. The latter's doc ("The step's gate and the cache's `exterior_ready` are one predicate...") now sits on the new test; the real owner has none.

## Impact
rustdoc and navigation only.

## Suggested Fix
Move each doc paragraph back onto its item.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
