# Fixed-state revalidation — 2026-08-11

## Result

The large direct fragment-shader cost survives removal of wall-clock simulation
feedback. On the `6c56e311` host, changing only `triangle.frag.spv` from the
shipped `6c56e311` module to the shipped `e414249f` parent module changed the
median 1,000-frame Prospector result from **14.93 ms to 6.28 ms** (**2.38×**).

This revalidates the direct SPIR-V cost. It does not rehabilitate the original
300-frame dynamic-harness rows: those absolute samples remain invalid because
wall-clock dt and wall-time-driven resource readiness were uncontrolled.

## Method

- Host source and ABI: `6c56e311`, extracted with `git archive`.
- Only swapped artifact: `crates/renderer/shaders/triangle.frag.spv`.
- Parent module SHA-256: `ec2d08a68da40b8e86faccc0284103c73cb7a9c99ca452cce93932e8ef8e0b79`.
- `6c56e311` module SHA-256: `795955d6d0953446fbbf77919e92bf495cdd71b1f9c371d286caa9bb4ef29e27`.
- Scene: FNV `GSProspectorSaloonInterior` on the RTX 4070 Ti host display.
- Contract: fixed `dt = 0`, no input/static camera, externally labelled
  `renderer-static`; variants were interleaved.
- Acceptance columns: 3,626 entities, 1,224 draws, 32 post-merge batches, and
  3 GPU calls in every accepted row.

The current HEAD host was deliberately not used for the swap. SPIR-V reflection
showed that `GpuInstance` and `GpuMaterial` have grown since July; loading the
old modules into today's host would be an ABI-invalid experiment even though
their descriptor bindings remain compatible.

The July host predates the Phase 0 scene-state hash, so this result is not a
substitute for the hash-backed four-boundary matrix. It is the cheapest decisive
Phase 2 test: fixed simulation plus identical final aggregate render state.

## Shipped-module swap

| module | wall-ms samples | median wall ms | median FPS | median fence ms | final state |
|---|---|---:|---:|---:|---|
| `6c56e311` shipped | 14.86, 14.93, 14.94 | **14.93** | 67.0 | 13.78 | 3626 / 1224 / 32b / 3c |
| `e414249f` shipped | 6.28, 6.28, 6.40 | **6.28** | 159.2 | 5.31 | 3626 / 1224 / 32b / 3c |

The new module adds 8.65 ms over the parent (+137.7%, a 2.38× ratio). This is
consistent with the July ~2.2× conclusion and much larger than run-to-run noise.

## Per-knob confirmation

The committed `6c56e311` GLSL is not buildable: the alpha-sensitive shadow arm
reads `alphaThreshold` and the alpha-blend flag from the opposite structures.
For the knob ladder only, those two field owners were corrected and the shader
was recompiled. That repaired-source baseline measured 14.88 ms in its initial
1,000-frame check versus 14.93 ms for the shipped module, establishing baseline
cost parity despite the expected byte mismatch.

Faster variants sometimes reached frame 1,000 before material/batch readiness
settled. Those rows were rejected. The accepted table uses 3,000 frames; every
row ended at 3,626 entities / 1,224 draws / 32 batches / 3 calls.

| compile-time configuration | FPS | wall ms | fence ms | wall-ms recovery vs baseline |
|---|---:|---:|---:|---:|
| repaired-source baseline | 68.6 | **14.57** | 13.50 | — |
| binary shadow query (pre-`6c56e311` semantics) | 123.0 | **8.13** | 7.12 | 6.44 ms (44.2%) |
| GI diffuse bounces 2 → 1 | 86.9 | **11.50** | 10.42 | 3.07 ms (21.1%) |
| GI path segments 6 → 2, bounces 2 | 78.1 | **12.80** | 11.75 | 1.77 ms (12.1%) |

The ordering matches the July table: the shadow-transmittance rewrite is the
largest term, the second diffuse bounce is next, and the path-segment ceiling
is the minor term. These deltas are sensitivity measurements and are not
additive because shadow traversal is also used at GI hit points.

## Interpretation

- **Survives:** `6c56e311`'s shipped fragment shader has a large direct cost
  under fixed simulation state; the ~2.2× magnitude and per-knob ranking are
  independently reproduced.
- **Still withdrawn:** the original dynamic-harness absolute rows and any
  conclusion that assumes frame 300 represented the same simulation/resource
  state without checking the forensic columns.
- **Still required:** the hash-backed four-boundary matrix. Phase 2 changes it
  from discovery to confirmation; it does not make that matrix unnecessary.
