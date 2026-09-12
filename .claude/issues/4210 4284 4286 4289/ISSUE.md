# #4210 — PERF-D9-2026-09-11-03: `gpu_timers.rs` still documents itself as a 16-bracket/32-query pool after #4052 made it 17/34

**Description**: #4052 correctly bumped `QUERIES_PER_FRAME` to 34, `active_bits` to `u32`, and added `BIT_GROUNDCOVER_BENCH`, but four prose counts in the same file's comments still say "32"/"sixteen".

**Suggested Fix**: Replace the four literals with current counts, or word them to reference `QUERIES_PER_FRAME` directly; add a `const _: () = assert!(...)` cross-check.

## Completeness Checks
- [x] **SIBLING**: Checked all four cited locations plus a fifth ("sixteen brackets" at line 266) — that one is historical prose describing the *pre-#4052* state accurately and was left alone.
- [x] **TESTS**: Added `doc_table_slot_count_matches_queries_per_frame`, verified against a temporarily-corrupted table.

**Disposition**: Fixed the module-doc summary line and two "sixteen"/"32-query" mentions to say "seventeen"/"34". Rather than a `const _: () = assert!(...)` (which can't see into a markdown comment), added a test that parses the module doc's own `| Slot | Bracket |` table and asserts its row count and numbering exactly match `QUERIES_PER_FRAME` — a real cross-check, not just corrected literals that can drift again. Verified by temporarily deleting one table row and confirming the test fails, then restored.

---

# #4284 — SF-2026-09-11-D8-03: Material::resolve_pbr's own contract-doc comment calls its NaN backstop unreachable, false since #2707 for 97.9% of Starfield meshes

**Severity**: MEDIUM
**Dimension**: Dimension 8 — NIFAL Canonical Material Translation for Starfield
**Location**: `crates/core/src/ecs/components/material.rs` (`Material::resolve_pbr`)
**Status**: NEW

**Description**: `resolve_pbr`'s contract-doc comment describes its NaN-sentinel backstop as a "future... backstop only" — false since #2707 made it live for Starfield's material-reference-stub case (97.9% of Starfield meshes in the audit's sampled corpus).

**Suggested Fix**: Update the contract-doc comment to state the arm is live and reachable, citing #2707 and the 97.9% figure.

## Completeness Checks
- [x] **SIBLING**: Checked the inline comment right at the classifier call site — it already correctly documents reachability ("but it is a real, live path... #2707"); only the outer contract-doc block needed correcting.
- [x] **TESTS**: N/A — documentation-only fix.

**Disposition**: Rewrote the outer doc block's misleading "sentinel-backstop for future... sources only" phrasing to state the arm is live today for Starfield, citing #2707 and the 97.9% figure, with an explicit warning against removing it as dead code.

---

# #4286 — SF-2026-09-11-D9-01: the BGSM merge arm assigns (rather than ORs) the greyscale-palette enable bit when the BGSM fills a role the NIF left empty, silently clearing a NIF-authored SLSF1 palette remap

**Severity**: MEDIUM
**Dimension**: Dimension 9 — BGSM/BGEM External Material Flow
**Location**: `byroredux/src/asset_provider/material/merge.rs:631-641`
**Status**: NEW

**Description**: When the BGSM fills a texture role the NIF left empty, `bgsm_greyscale_lut_enabled` is plain-assigned from the BGSM's own bit instead of OR'd — clobbering a NIF-authored SLSF1 enable bit (#3897) whenever the BGSM's own bit happens to be false. Reachable on any Skyrim-layout BGSM mesh (wire slot 3 is Height there, never GreyscaleLut).

**Suggested Fix**: Change the `is_none()` branch to OR the bit in, consistent with #3898's "neither source may silently disable the other's remap" invariant.

## Completeness Checks
- [x] **CANONICAL-BOUNDARY**: N/A — the fix stays inside the BGSM merge step, not `translate_material`/`resolve_pbr`.
- [x] **SIBLING**: Found and fixed the identical pattern on `bgsm_greyscale_lut_color` in the same branch — the issue's evidence only quoted `bgsm_greyscale_lut_enabled`, but `bgsm_greyscale_lut_color` is set by the same NIF SLSF1 forwarding (#3897) and was equally reachable to the same clobber.
- [x] **TESTS**: 1 new regression test, verified against a temporary revert.

**Disposition**: Changed both `bgsm_greyscale_lut_enabled` and `bgsm_greyscale_lut_color` in the `is_none()` branch from assignment to OR, matching the `nif_supplied_greyscale_lut` branch immediately below and #3898's stated invariant. Added `bgsm_winning_the_slot_does_not_clear_a_nif_enabled_remap`, the sibling of the existing `bgsm_without_palette_bit_does_not_disable_a_nif_enabled_remap` test but for the `is_none()` branch instead of the `nif_supplied_greyscale_lut` one. Verified it fails against a temporarily-reverted (assignment) implementation, then restored.

---

# #4289 — SF-2026-09-11-D9-04: the #[must_use] MergeOutcome is let-_-'d at all four production call sites — the PresenceOnly signal #2709 created has no telemetry sink on Starfield ~100% of materials

**Severity**: LOW
**Dimension**: Dimension 9 — BGSM/BGEM External Material Flow
**Location**: `byroredux/src/asset_provider/material/merge.rs`, `byroredux/src/cell_loader/refr.rs` — four production call sites
**Status**: NEW

**Description**: All four production call sites of the merge functions returning `MergeOutcome` discard it with `let _ = ...`. `PresenceOnly` — load-bearing for the D8-02 glass-promotion overload — is produced constantly (close to 100% of Starfield materials via the CDB-gated `.mat` path) but observed nowhere.

**Suggested Fix**: Add a debug-server counter or `debug!`-level log line at one or more of the four call sites that records `MergeOutcome` outcomes.

## Completeness Checks
- [x] **SIBLING**: N/A.
- [x] **TESTS**: 1 new test (`trace_merge_outcome_does_not_panic_for_any_variant`); existing `MergeOutcome`-producing tests (69) verified passing unchanged, confirming the diagnostic changed no return-value behavior.

**Disposition**: Rather than editing all four call sites (which would also have needed a second exported helper — blocked by this file's own `single_boundary_tests::merge_external_material_is_the_only_exported_fn_in_this_file` invariant, deliberately keeping `merge_external_material` the one way a sidecar reaches a material), added a private `trace_merge_outcome` sink and wired it into `merge_external_material` itself at all three of its `PresenceOnly`-producing return points (the two `apply_cdb_pbr_fallback` call sites and the final `touched`-based branch). `log::trace!`, not `debug!`, since this fires for close to 100% of Starfield materials — anything louder would flood a debug-level log with an expected, near-universal outcome rather than surfacing an anomaly. The repo has no log-capture test harness, so the emitted line itself isn't asserted; the new test pins that the sink is callable for every variant without panicking, and the full existing merge test suite (69 tests) confirms wiring it in changed no outcome value.
