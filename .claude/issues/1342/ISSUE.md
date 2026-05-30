# #1342 — D2-01: FO3/FNV WEAP DNAM mislabels Min Spread / Spread as ap_cost / min_spread

_Snapshot as filed from AUDIT_FNV_2026-05-30 (d2-01). GitHub is authoritative for live state — query `gh issue view 1342 --json state`._

**Severity**: MEDIUM · **Dimension**: ESM Record Parser · **Source**: AUDIT_FNV_2026-05-30 (D2-01)

> Label note: repo has no `esm` domain label; filed under `import-pipeline`. Consider adding an `esm` label.

**Location**: `crates/plugin/src/esm/records/items.rs:205-214`

**Description**: The FO3/FNV WEAP DNAM decode stores the u32 at offset 16 as `ap_cost` and the value at offset 20 as `min_spread`, but the FO3/FNV WEAP DNAM layout has **Min Spread (f32)** at offset 16 and **Spread (f32)** at offset 20. The field read as `ap_cost` is actually a float (~0.024 for the Varmint Rifle) reinterpreted as a garbage u32.

**Evidence**: items.rs:212-214 — `r.skip_or_eof(15); // pad up to ap_cost at offset 16` then `ap_cost = r.u32_or_default(); min_spread = r.f32_or_default();`. Dumping the Varmint Rifle's raw DNAM shows the offset-16 u32 is a float ~0.024 (Min Spread), not an AP cost.

**Impact**: Latent — these fields have no runtime consumer yet and the integration test asserts nothing about them — but the stored values are wrong and would surface the moment AP-cost / spread is wired to gameplay.

**Suggested Fix**: Re-map per the FO3/FNV WEAP DNAM layout (Min Spread f32 @16, Spread f32 @20; locate the real Action Points cost field elsewhere in DNAM) and add a Varmint-Rifle assertion to the WEAP parse test.

## Completeness Checks
- [ ] **SIBLING**: Verify the full DNAM field map against the FO3/FNV layout (other fields after offset 20 may also be shifted).
- [ ] **TESTS**: Add a Varmint-Rifle (or known WEAP) assertion pinning Min Spread / Spread / AP cost values.
