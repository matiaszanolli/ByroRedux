# #3742 — TD2-2026-08-30-02: the 64-entry `BLUE_NOISE_RANKS` table is byte-identical in two shaders that already share the include it belongs in

**Labels**: bug, renderer, low, tech-debt, shaders

---

- **Severity**: LOW
- **Dimension**: 2 — Duplication
- **Location**: `crates/renderer/shaders/composite.frag:258-267` and `crates/renderer/shaders/volumetrics_inject.comp:1246-1255`
- **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-30.md` (`TD2-2026-08-30-02`), HEAD `64f64480`

## Description

Both files each declare:

```glsl
const uint BLUE_NOISE_RANKS[64] = uint[64](
     0u, 41u, 11u, 59u,  2u, 40u, 10u, 32u,
    ...
    63u, 16u, 57u, 27u, 37u, 19u, 56u, 30u
);
```

Diffed line-by-line: the two tables are **byte-identical**. Only the consuming function
differs (`preResolveDither()` vs `blueNoiseRank(ivec2, int)`), and each consumer's tiling
offsets are its own business — the *table* is the shared constant.

## The consolidation site already exists and both files already use it

Both `#include "include/shader_constants.glsl"` (`composite.frag:8`,
`volumetrics_inject.comp:33`). Move the array into a header — either
`shader_constants.glsl` via `crates/renderer/src/shader_constants_data.rs` (the documented
single source for every shader constant, `include!`d by both `shader_constants.rs` and
`build.rs`) or a new `include/blue_noise.glsl` — and delete both copies. The name is
currently absent from `shader_constants_data.rs` (verified: 0 hits).

## Impact

An 8×8 void-and-cluster rank table is exactly the kind of value that must never diverge:
if one copy is regenerated and the other is not, the composite dither and the froxel
jitter fall out of phase and produce **correlated banding that looks like a denoiser bug,
not a constants bug**. Effort: trivial.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders declaring their own copies of shared tables)
- [ ] **TESTS**: A regression test pins this specific fix — extend `shader_constants.rs`'s provenance assertions to reject a re-declaration of `BLUE_NOISE_RANKS`
