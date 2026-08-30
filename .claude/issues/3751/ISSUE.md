# #3751: SPT-2026-08-30-D3-03: CNAM is 8 floats on all three games — the documented "5 x f32 on Oblivion" split is fiction, and a unit test pins it

**Labels**: documentation, low, legacy-compat, game:oblivion, esm-plugin, speedtree, doc-rot
**Filed**: 2026-08-30 (audit-publish)

---

**Report**: `docs/audits/AUDIT_SPEEDTREE_2026-08-30.md` · **Severity**: LOW · **Dimension**: 3 (TREE→Billboard Wiring)
**Game affected**: Oblivion, FO3, FNV

## Location
- `crates/plugin/src/esm/records/tree.rs` — the module doc (`:25-28`), the `canopy_params` field doc (`:92-96`), the parse comment (`:160-170`), and the test `parse_oblivion_short_cnam_no_bnam_no_pfig` (`:277-300`)
- `crates/spt/src/import/mod.rs` — the "5 × f32 on Oblivion / 8 × f32 on FO3/FNV" claim

## Description
Three docstrings and one test assert that `CNAM` carries 5 floats on Oblivion and 8 on FO3/FNV. Measured, the payload is **32 bytes / 8 floats on 142/142 Oblivion, 9/9 FO3 and 3/3 FNV records — there is no split.**

The parser itself is length-tolerant (`while let Ok(v) = r.f32()`), so nothing mis-parses today; the defect is that the documented input contract is wrong, and the only test covering the Oblivion shape is a synthetic fixture matching no vanilla record. Its name, `parse_oblivion_short_cnam_no_bnam_no_pfig`, asserts **two false vanilla facts in one identifier** — the short CNAM, and (per the BNAM finding filed alongside this) the absent BNAM.

## Evidence
`CNAM` payload-length histogram, `.spt`-bearing TREE records: Oblivion `{32: 142}`, FO3 `{32: 9}`, FNV `{32: 3}`. **No other length occurs.** The `canopy_params: Vec<f32>` field length therefore reads 8 everywhere.

Re-verified 2026-08-30 that all five prose sites still carry the 5-vs-8 claim, and the test still asserts `tree.canopy_params.len() == 5, "Oblivion CNAM is 5 × f32"`.

## Impact
No live parse defect — `canopy_params` is parse-but-don't-consume behind the #3190 gate. The cost is entirely forward-looking and lands on that gate: **a wind decoder written against "5 floats on Oblivion" will index the wrong slots on 100 % of Cyrodiil trees.**

The field *semantics* remain **unattested** and this audit proposes none; only the count is measured.

## Related
#3190 (the deferred consumer); #3276 (the last CNAM docstring repair, which corrected the *wind-source* claim but left the 5-vs-8 claim standing); the BNAM/MODB finding filed alongside this one (same record, same class of corpus-premise error).

## Suggested Fix
Replace the 5/8 claim with the measured 8-everywhere fact in all five places, and rename/retarget the Oblivion test to a **real vanilla shape** (8-float CNAM, BNAM present, no OBND, MODB present) so it pins the corpus instead of a fiction. Keep the length-tolerant reader — it is the right shape for mod content — but do not describe the tolerance as covering a split that does not exist.

## Completeness Checks
- [ ] **SIBLING**: all five prose sites across the two crates plus the test name must change together
- [ ] **TESTS**: the retargeted test must pin the measured vanilla shape, not a synthetic one
