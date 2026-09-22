# PAR-D5-2026-09-21-04: FaceGen EGT and TRI parsers mis-describe their formats (latent: no consumer)

Labels: low,bug,import-pipeline

## Description
- **EGT** (`crates/facegen/src/egt.rs:13-27`, `:142-151`). The FaceGen SDK manual gives each mode as `float s, <image> r, <image> g, <image> b` with `<image> = (signed char * R) * C`: planar and signed. The parser instead pushes interleaved `[bytes[o], bytes[o+1], bytes[o+2]]` triples and its own doc comment describes an offset-128 unsigned reading. The SDK header order is R, C, S, A, Texture Basis Version (vanilla 256, 256, 50, 0, 81); the parser calls A and the basis version `unknown_a`/`unknown_b` and sizes the file from S only.
- **TRI** (`crates/facegen/src/tri.rs:17-36`, `:80-98`). The SDK header order is V, T, Q, LV, LS, X, ext, Md, Ms, K. `TriHeader` stores X as `num_modifier_vertices`, ext as `num_modifiers`, Md as `num_uv_coords` and Ms as `num_quads`; Q, LV, LS and K become unknown words. Vanilla `headhuman.tri` words: 1211, 2294, 0, 0, 0, 1211, 1, 38, 8, 238. The module doc and the code also disagree with each other on the field order.

Verified unchanged at HEAD `ee6d3fb39`: `egt.rs`'s pixel loop is still interleaved `[bytes[offset], bytes[offset+1], bytes[offset+2]]`; `tri.rs`'s field assignment order is unchanged.

## Evidence
Probe `egt-stats`, vanilla `headhuman.egt`, first 10 modes, first third read as `i8`:
- lag-1 correlation 0.993 > lag-3 0.952 (planar; interleaving would make lag 3 the same-channel neighbour instead);
- lag-256 (next row) 0.972 > lag-768 0.840;
- 58.7% of bytes are within 16 of 0x00/0xFF versus 0.8% near 0x80, i.e. zero-centred signed chars, not offset-128 unsigned.

## Impact
Latent — there is no consumer (#3544) today — but the future FGTS compositor and lip-sync `.tri` work would inherit wrong decodes and misnamed fields if built directly on the current code without re-deriving the format.

## Related
#3544 (the no-consumer tracker for both formats), PAR-D5-2026-09-21-01 (sibling FaceGen finding, same crate — the live EGM decode bug)

## Suggested Fix
Decode EGT as planar `i8` planes (R*C each) and rename the TRI header fields to the SDK order. Add the vanilla header words as a real-data assertion.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D5-2026-09-21-04)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix