# #3152 — LC-D2-01: mesh-bound water gates `blend_normals` on NIF flag bit 16, which the format does not define — every vanilla `BSWaterShaderProperty` mesh flips the canonical default to `false`

**Finding**: LC-D2-01
**Labels**: bug, medium, legacy-compat
**Filed**: 2026-08-20 · `/audit-publish` · HEAD `bb0b92f2`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/3152

---

- **Severity**: MEDIUM
- **Dimension**: LEGACY_COMPAT Dim 2 — NIFAL, canonical NIF→ECS translation contract (mesh-water slice, new in session 70)
- **Location**: `byroredux/src/material_translate.rs:135-147` (the flag-gate block inside `water_material_from_mesh`); pinned by `byroredux/src/material_translate.rs:756-796`
- **Status**: NEW

> ### ⚠️ NO-ANALOGY WARNING — READ BEFORE TOUCHING EITHER BIT
> There are **two different water flag bits** in play across the current WATR/WATAL work, in **two different file formats**, and they must not be reasoned about together:
>
> | | Format | Bit | Verdict |
> |---|---|---|---|
> | `WATR.FNAM` bit `0x10` | ESM record | 4 | **CORRECT — empirically verified** (`DefaultWaterFlow` 0x08 vs `DefaultWaterFlowBlend` 0x18). See #3104–#3110. |
> | `BSWaterShaderProperty` `blend_normals` bit **16** | NIF block | 16 | **UNDEFINED in the format — this issue.** |
>
> Fixing one by analogy with the other breaks working code. The `FNAM` decode is not evidence for the NIF bit, and correcting the NIF bit must not touch `water.rs:1309`.

## Description

`water_material_from_mesh` copies the parsed `BSWaterShaderProperty.water_shader_flags` word onto `WaterMaterial::shader_flags` and then, whenever that word is non-zero, decides:

```rust
// byroredux/src/material_translate.rs:141-147
if water.shader_flags != 0 {
    const USE_REFLECTIONS: u32 = 1 << 6;
    const BLEND_NORMALS: u32 = 1 << 16;
    if water.shader_flags & USE_REFLECTIONS == 0 {
        water.effect_controls[2] = 0.0;
    }
    water.blend_normals = water.shader_flags & BLEND_NORMALS != 0;
}
```

The authoritative NIF spec defines that word as the `WaterShaderPropertyFlags` bitfield, and it has **fourteen** options — bits 0 through 13 — with a format default of `0xC4`. **Bit 16 is not part of the file-side enum**, so no authored NIF can set it.

The consequence is not "a flag is ignored". Because `WaterMaterial::default().blend_normals` is `true` (`crates/core/src/ecs/components/water.rs:311`), the gate **inverts** the canonical default for every mesh whose flag word is non-zero — i.e. for every mesh that authored anything at all, including the vanilla default `0xC4`.

The in-code comment attributes bits 15/16 to CommonLibSSE. CommonLibSSE describes the **runtime** object, not the wire format the parser reads. The two enums are being conflated at the translate boundary.

## Evidence

`/mnt/data/src/reference/nifxml/nif.xml`, the authoritative spec (precedence over the Gamebryo 2.3 source per project convention):

```xml
<field name="Water Shader Flags" type="WaterShaderPropertyFlags" default="0xC4" />   <!-- :6705 -->

<bitflags name="WaterShaderPropertyFlags" storage="uint" prefix="BSWSP" versions="#SKY_AND_LATER#">   <!-- :6677-6693 -->
    bit 0 DISPLACEMENT   bit 1 LOD        bit 2 DEPTH       bit 3 ACTOR_IN_WATER
    bit 4 ACTOR_IN_WATER_IS_MOVING        bit 5 UNDERWATER  bit 6 REFLECTIONS
    bit 7 REFRACTIONS    bit 8 VERTEX_UV  bit 9 VERTEX_ALPHA_DEPTH
    bit 10 PROCEDURAL    bit 11 FOG       bit 12 UPDATE_CONSTANTS  bit 13 CUBEMAP
</bitflags>
```

The default `0xC4` is `DEPTH | REFLECTIONS | REFRACTIONS` — non-zero, so the gate fires; bit 16 is clear, so `blend_normals` becomes `false`.

The parse chain is intact and does not truncate the word — `crates/nif/src/blocks/shader.rs:545` reads a full `u32`, `crates/nif/src/import/material/dedicated_shader.rs:619` forwards it, `material_translate.rs:96` copies it — so the word reaching the gate is exactly the authored one.

**The two unit tests that "pin" this behaviour construct values no authored NIF can produce.** `mesh_water_honors_authored_optical_flag_gates` and `mesh_water_honors_authored_reflection_and_blend_flags` build `water_shader_flags = 1 << 16` and `(1 << 6) | (1 << 16)`, and `assert!(!water.blend_normals)` at `material_translate.rs:789` locks in the wrong answer for the realistic case. The suite is green against synthetic input vanilla data never supplies.

**Secondarily, the same block honours only bit 6.** `DISPLACEMENT` (0), `DEPTH` (2), `REFRACTIONS` (7), `PROCEDURAL` (10), `FOG` (11) and `CUBEMAP` (13) are parsed and dropped — even though `docs/engine/watal.md` §6 states the canonical type *"still carries DISPLACEMENT/LOD/DEPTH/REFLECTIONS/REFRACTIONS flags so the per-game translate can disable rays for opaque waterfalls … without a shader per-game branch"*. That is precisely the case the spec calls out and the code does not implement: an opaque waterfall authored with `REFRACTIONS` clear still gets refraction rays.

## Impact

Every dedicated water mesh in Skyrim and later — waterfall sheets, cascade panels, mill races, the localized water NIFs the cell loader does not spawn as planes — loses authored multi-layer normal blending, because the canonical `true` is flipped to `false` by a bit that is structurally always clear. Visually this reads as flat, single-layer water on exactly the assets that need chop most.

It is silent: no warning, and the test suite is green.

Blast radius is bounded to mesh-bound water — cell and worldspace planes take the `env_translate` path, which sets `blend_normals` from the WATR `FNAM` byte and **is correct** — which is why this is MEDIUM and not HIGH.

## Related

- #3104–#3110 — the WATR-side arbitration. **Adjacent but disjoint**; see the NO-ANALOGY box above.
- LC-D5-02 (filed separately) — the same slice is an undeclared second `WaterMaterial` producer.
- `docs/engine/watal.md` §6.

## Suggested Fix

1. Replace the two ad-hoc constants with the spec's `WaterShaderPropertyFlags` bit names.
2. Decide `blend_normals` from a bit that **exists in the file enum** — or, if no file bit carries that meaning, drop the gate entirely and let the canonical `true` stand rather than inverting it from an always-clear bit.
3. Rewrite the two tests to use the format default `0xC4` as the realistic case.
4. While there, wire `REFRACTIONS` (bit 7) and `DEPTH` (bit 2) into the canonical flags so `watal.md` §6's opaque-waterfall claim becomes true.

---
*Filed from `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-20.md` (LC-D2-01). Verified against HEAD `bb0b92f2` and against `nif.xml` before filing.*

## Completeness Checks
- [ ] **NO-ANALOGY**: the WATR-side `FNAM` bit `0x10` decode (`crates/plugin/src/esm/records/misc/water.rs:1309`) is empirically correct and must NOT be changed as part of this fix — different bit, different format
- [ ] **SIBLING**: every other place that reads `water_shader_flags` / `WaterMaterial::shader_flags` checked against the `nif.xml` bit list, including the shader-side consumers
- [ ] **CANONICAL-BOUNDARY**: the flag→canonical decision stays in `water_material_from_mesh` at the NIFAL parser→canonical boundary — never re-derived at render time and never pushed into `water.frag`. See `/audit-nifal`.
- [ ] **TESTS**: the pinning tests use the authored format default `0xC4`, not a synthetic `1 << 16` no NIF can produce
