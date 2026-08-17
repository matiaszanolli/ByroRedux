# FO4-D5-06: FO4 slot 3 is a greyscale-to-palette gradient, routed into the POM height role

**Issue**: #2997
**Severity**: HIGH
**Dimension**: 5 — shader flags / slot routing
**Labels**: `high,nif-parser,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_FO4_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_FO4_2026-08-16.md` (Dimension 5 — shader flags / slot routing).

**Location**: `crates/nif/src/import/material/slot_role.rs`:118-121; consumed at `crates/nif/src/import/material/dedicated_shader.rs`:144-176 (`TextureRole::Height => &mut info.parallax_map`)

## Description

`slot_to_role(ty, 3, _)` returns `Some(TextureRole::Height)` for every shader type except `FACE_TINT`. That arm's evidence is **Skyrim-only** (nif.xml calls slot 3 "Height/Parallax", and #2694 measured FaceTint against `Skyrim - Meshes0.bsa`).

On FO4, slot 3 carries the **greyscale-to-palette LUT** for the `GREYSCALE_TO_PALETTE_COLOR` / `_ALPHA` feature — a colour gradient strip, not a height field.

## Evidence

Slot-3 occupancy and what actually occupies it:

| Archive | type 0 | type 1 | type 2 | type 6 |
|---|---|---|---|---|
| `Fallout4 - Meshes.ba2` | 1,990 | 1,415 | 731 | 1,320 |
| `Fallout4 - MeshesExtra.ba2` | 10,333 | 15,514 | — | — |

**31,303 properties** total. Every sampled path is unmistakably a palette gradient:

```
Architecture/Buildings/LargeBrickGrad01.dds
Architecture/Buildings/Bricks01Grad01.dds
Architecture/Buildings/PaintedWoodGrad01.dds
Architecture/Buildings/Hightech/HitTechMetalPanel_01LGrad.dds
Interiors/HighTech/Grads/FlatMetalPanels01Grad.dds
SetDressing/Crates/CrateLarge01LGrad.dds
Vehicles/Automotive/Rust01LGrad.dds
Actors/Character/Hair/HairColor_Lgrad_d.dds
textures\Actors\PowerArmor\PA_palette_d.dds
```

Corroborating engine-side signal: `fo4_slsf1::GREYSCALE_TO_PALETTE_COLOR` (`0x10`) and `_ALPHA` (`0x20`) already exist at `crates/nif/src/shader_flags.rs`:163-164.

**The correct canonical role already exists and is structurally unreachable.** `MaterialTextureSet::greyscale_lut` (`crates/nif/src/import/types.rs`:328, bindless index 15) has a live renderer consumer — `byroredux/src/cell_loader.rs`:289-294 sets `EFFECT_PALETTE_COLOR` / `EFFECT_PALETTE_ALPHA` from it. But the `TextureRole` enum (`slot_role.rs`:37-54) has **no `GreyscaleLut` variant**, so the shared table cannot name it; today the only producer of `greyscale_lut` is the BGSM/BGEM merge.

Re-verified 2026-08-17: `3 => match shader_type { FACE_TINT => Detail, _ => Height }` is present and unchanged.

## Impact

The slot-3 path writes `MaterialInfo::parallax_map` → `GpuMaterial::parallaxMapIndex`. `crates/renderer/shaders/triangle.frag`:208 gates POM on `mat.parallaxMapIndex != 0u && (dbgFlags & DBG_BYPASS_POM) == 0u` — **no material-kind, shader-flag or height-scale check** — and `GpuMaterial::default()` (`crates/renderer/src/vulkan/material.rs`:369) sets `parallax_height_scale = 0.04`, so the branch is unconditionally live.

FO4 brick facades, painted wood, hi-tech metal panelling, vehicles, crates, power armour and hair therefore **ray-march parallax occlusion against the red channel of a colour palette**, displacing UVs by an unrelated signal. Simultaneously the authored palette remap is lost from the NIF side.

Same defect class as #2694 (POM over a face complexion map), on FO4, **31,303 properties wide** — reaching the densest, most-visible architecture set in the game.

## Suggested Fix

Add a `TextureRole::GreyscaleLut` variant routed to `MaterialTextureSet::greyscale_lut`, and make the slot-3 arm game-aware.

Since `slot_to_role` deliberately takes no game parameter, the cleanest seam is the one `normalize_shader_type` already uses — **normalise at the call site in `dedicated_shader.rs`**, which has `bsver` in scope, rather than widening the shared table's signature.

## Related

- #2694 (CLOSED — the Skyrim FaceTint slot-3 fix that established the method, and whose generalisation to FO4 is this bug)
- #2108 / `bgsm_greyscale_lut_enabled` (the existing palette-remap consumer)
- #2999 (FO4-D5-08 — same "Skyrim evidence generalised to FO4" root cause)

## Completeness Checks
- [ ] **SIBLING**: Every `slot_to_role` arm whose evidence is a Skyrim archive measurement re-checked against FO4 (slots 4/5/7 are already known — see #2998, #2999)
- [ ] **CANONICAL-BOUNDARY**: The game-aware normalisation lives at the parser→`Material` boundary (`dedicated_shader.rs`), never in the shader or renderer
- [ ] **NO-DEAD-ROLE**: `greyscale_lut` reachable from the NIF path, not only from the BGSM merge
- [ ] **POM-GATE**: Consider whether `triangle.frag`'s unguarded POM branch deserves a material-kind check independently
- [ ] **TESTS**: A regression test asserts an FO4 slot-3 binding lands in `greyscale_lut`, not `parallax_map`

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 2997 --json state` when live state is needed.*
