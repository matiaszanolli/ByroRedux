# PAR-D5-2026-09-21-01: FaceGen .egm morph deltas are decoded as IEEE half-floats, but the format stores per-morph-scaled signed 16-bit integers

Labels: high,bug,import-pipeline,game:fo3,game:fnv,game:oblivion

## Description
`crates/facegen/src/egm.rs:144-151` decodes each delta component with `half_to_f32(u16)`. The FaceGen SDK file-format manual specifies, per mode, a `float` scale x, then for each vertex "3 signed short m. The actual morph values should be m * x". PyFFI's EGM format (scale = n/32768.0, integer components) agrees.

- `i16` and `f16` are both 2 bytes, so the exact-size check (`egm.rs:18-28`) and the real-data test both pass on wrongly-typed data — the bug is invisible to size/shape validation.
- The evaluator's rationale that "FaceGen used NaN as a 'no displacement' sentinel" (`crates/facegen/src/eval.rs:20-30`) describes a symptom of this mis-decode, not an intentional design: those "NaN" bit patterns are small negative int16s reinterpreted as half-floats.
- `egm.rs:15`'s doc comment "num_vertices verified == base head NIF's vertex count" is also false — EGM V = TRI V + K (vanilla `headhuman.tri`: V=1211, K=238 -> 1449). The consumer's "best-effort prefix" over the first 1211 (`crates/facegen/src/lib.rs:79-118`) is therefore the right base-vertex mapping; only the numeric type of the delta components is the defect.

Verified unchanged at HEAD `ee6d3fb39`: `egm.rs:147-149` still decodes `dx`/`dy`/`dz` via `half_to_f32(u16::from_le_bytes(...))`.

## Evidence
Probe `egm-stats`, vanilla FNV `meshes\characters\head\headhuman.egm`, 1449 verts, 50+30 morphs:

```
morphs whose max |int16| >= 30000: 80 of 80   (all exactly 32767: full-range int16 quantisation)
f16 non-finite: 32,456 (9.33%), of which int16 in [-1024,-1]: 32,319   (the rest are int16 31744..32767)
int16 sign split: pos 170,754 / neg 170,898 / zero 6,108
finite components 315,304: mean |true delta| 0.00727, mean |f16 decode - true| 0.01349 (1.86x); 16.4% off by > 2x
```

Same signature on FNV `cowboyhat.egm` (675 v), `hockeymask.egm` (1424 v) and Oblivion `armor\chainmail\m\helmet.egm` (779 v): 80/80 near-full-range, 7.7-13.0% NaN. Negative deltas in [-32768, -1025] decode in inverted magnitude order under the wrong interpretation: -1025 -> -65504*scale, and -32768 -> -0.

## Impact
Every runtime-FaceGen NPC head, via `npc_spawn/resumable.rs:1193-1239` -> `apply_morphs` (FO3/FNV, and Oblivion where a recipe exists), is deformed by wrong deltas. About 9% of components are dropped as NaN/Inf and the rest have nonlinear, partly inverted magnitudes. `docs/feature-matrix.md:76` lists FaceGen morphs (check) for FO3/FNV. The real-data test only prints the non-finite count, so nothing catches this.

## Related
#3048 (evaluator NaN/overflow guard), #2599 (the facegen `half_to_f32` copy exists only for this decode), #3544, PAR-D5-2026-09-21-04 (sibling FaceGen finding — EGT/TRI mislabeling, same crate)

## Suggested Fix
- Decode `i16::from_le_bytes` x morph `scale` (`EgmMorph.deltas` can then hold final displacements directly).
- Delete the facegen `half_to_f32` and its #2599 pin, and fix the format docs.
- Make `parse_vanilla_headhuman_egm` assert zero non-finite components and every morph's max |raw| near 32767.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D5-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix