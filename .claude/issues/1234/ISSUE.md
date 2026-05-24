# #1234 — REN-D13-NEW-09: caustic_splat.comp gate uses magic literal 4u instead of INSTANCE_FLAG_CAUSTIC_SOURCE macro

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-23_DIM13.md`
**Severity**: LOW
**Dimension**: Caustic Splat / Shader-Constants lockstep

## Symptom

The caustic-source gate at [crates/renderer/shaders/caustic_splat.comp:164](https://github.com/matiaszanolli/ByroRedux/blob/main/crates/renderer/shaders/caustic_splat.comp#L164) inlines the bit value:

```glsl
uint instIdx = meshId - 1u;
uint flags = instances[instIdx].flags;
if ((flags & 4u) == 0u) return;   // ← magic literal
```

The file already `#include`s `shader_constants.glsl` ([line 7](https://github.com/matiaszanolli/ByroRedux/blob/main/crates/renderer/shaders/caustic_splat.comp#L7)) which exposes the symbolic form:

```c
#define INSTANCE_FLAG_CAUSTIC_SOURCE 4u
```

(auto-generated from [`shader_constants_data.rs:86`](https://github.com/matiaszanolli/ByroRedux/blob/main/crates/renderer/src/shader_constants_data.rs#L86) via `build.rs`). Sibling shaders consistently use the symbolic form — `triangle.vert:174`, `triangle.frag:809 / :872 / :917 / :1029 / :1520` all read `(inst.flags & INSTANCE_FLAG_*) != 0u`.

## Cause

The line predates the shader-constants generator (#1119 / #1162) and was missed in the subsequent sweep that converted sibling shaders to the symbolic form.

## Impact

Lockstep drift potential. If `INSTANCE_FLAG_CAUSTIC_SOURCE` ever moves to a different bit position (e.g., a new flag claims bit 2 and CAUSTIC_SOURCE shifts to bit 6):

- The Rust ↔ generated-define drift is caught by `assert_eq!(INSTANCE_FLAG_CAUSTIC_SOURCE, SB_CAUSTIC_SOURCE)` at [`shader_constants.rs:313-320`](https://github.com/matiaszanolli/ByroRedux/blob/main/crates/renderer/src/shader_constants.rs#L313-L320).
- But the bare `4u` literal in `caustic_splat.comp` keeps reading bit 2 — silently severing the gate.
- The caustic dispatch would then run for whatever instances happen to have bit 2 set under the new scheme (or never fire at all if no instance sets that bit).

Same class of regression as #1099 (`CAUSTIC_FIXED_SCALE` clamp had a magic `4.0e7` literal unanchored from the named constant — a scale change would silently misalign).

## Fix

One-line replacement at `caustic_splat.comp:164`:

```glsl
if ((flags & INSTANCE_FLAG_CAUSTIC_SOURCE) == 0u) return;
```

Recompile SPIR-V (`crates/renderer/shaders/caustic_splat.comp.spv`).

## Optional defense-in-depth

Extend the `shader_constants::tests` module with a positive-side check that greps `caustic_splat.comp` for the symbolic form. Mirrors the existing `triangle_frag_dbg_bits_not_redeclared` negative-side check at [`shader_constants.rs:189-211`](https://github.com/matiaszanolli/ByroRedux/blob/main/crates/renderer/src/shader_constants.rs#L189-L211).

## Regression Risk

NONE — the macro and the literal evaluate to the same value today; the patch is a no-op at the SPIR-V level. The change tightens future lockstep, doesn't alter present behavior.

## Related

- #1099 (CLOSED) — same "magic literal unanchored from named constant" class of finding, fixed for `CAUSTIC_FIXED_SCALE`.
- #1162 (CLOSED) — sibling `DBG_*` redeclaration prevention via `triangle_frag_dbg_bits_not_redeclared` test; the optional defense-in-depth above mirrors that pattern.

## Completeness Checks

- [ ] **UNSAFE**: N/A — pure GLSL edit
- [ ] **SIBLING**: scan other compute shaders (`svgf_temporal.comp`, `taa.comp`, `cluster_cull.comp`, `skin_vertices.comp`, `skin_palette.comp`, `volumetrics_inject.comp`, `volumetrics_integrate.comp`, `bloom_downsample.comp`, `bloom_upsample.comp`, `ssao.comp`) for the same `flags & <literal>u` antipattern. The `triangle.{vert,frag}` siblings are already in the symbolic form.
- [ ] **DROP**: N/A — no Vulkan-object lifecycle change
- [ ] **TESTS**: optional — add a `shader_constants::tests` positive-side check that `caustic_splat.comp` contains `INSTANCE_FLAG_CAUSTIC_SOURCE` and does NOT contain `flags & 4u` (or any bare-literal pattern on the `flags` mask)
