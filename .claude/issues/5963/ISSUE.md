# SKY-D1-2026-10-17-01: tangent.rs comments still describe the removed Z-up copy; 'bit-identical' claim is wrong at the float32 last bit (#5793 follow-up)

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/5963 (snapshot as filed 2026-10-17; GitHub is authoritative for current state)

**Source**: `docs/audits/AUDIT_SKYRIM_2026-10-17.md` (report ID SKY-1-01) · **Severity**: LOW (doc rot) · **Dimension**: 1 · **Game**: Skyrim SE

## Description
After #5793 (`89a0b6ccd`) the Z-up `synthesize_tangents` is a wrapper over `synthesize_tangents_yup`, which is the only tangent producer, but `crates/nif/src/import/mesh/tangent.rs` still describes the removed copy:
1. `:307-313` (degenerate branch of `synthesize_tangents_yup`) still describes a "documented divergence" from the Z-up flavor's permutation (AUDIT_INCREMENTAL_2026-05-22 ID-4); there is no Z-up flavor left to diverge from, and the wrapper's own doc (`:141-147`) says the divergence is retired.
2. `:385-390` (`fallback_orthogonal_seed` doc) says "both producers' comments".
3. `:136-139` claims converting inputs first is "bit-identical" to the old accumulate-in-Z-up path. The per-triangle normalize now sums `x^2+z^2+y^2` instead of `x^2+y^2+z^2`, and float32 addition is not associative. A float32 check over 100,000 random triples found 22,786 where the two orders differ, so output can differ by about 1 ulp.

## Impact
Documentation only; no guard compares the wrapper against the old body (`tangent_convention_tests` check properties). The risk is an exact-equality golden test written on the strength of "bit-identical".

## Suggested fix
Delete the divergence paragraph at `:307-313`; "both producers'" -> "the producer's" at `:389`; reword `:138` to "mathematically identical (exact component permutation); float32 may differ in the last ulp".

Related: #5793, #2828, #3176, #2632 (closed). Also noted by the same audit: the module docs call the synthesis "Mikkelsen" but it is Lengyel / nifly `CalcTangentSpace` (`import/mesh/mod.rs:6`, `tangent.rs:3`, CLAUDE.md).

## Completeness Checks
- [ ] **SIBLING**: The "Mikkelsen" label in `import/mesh/mod.rs:6`, `tangent.rs:3` and CLAUDE.md corrected in the same pass
