# NIFAL-D8-2026-08-20-01: texture_slot_layout is assigned on only one of four shader-property branches, so FO4+ effect/sky/water meshes resolve REFR texture overrides through the Skyrim table

Issue: https://github.com/matiaszanolli/ByroRedux/issues/3186
Finding: NIFAL-D8-2026-08-20-01
Labels: medium,nif-parser,bug
Source: docs/audits/AUDIT_NIFAL_2026-08-20.md

Filed from `docs/audits/AUDIT_NIFAL_2026-08-20.md` (Dimension 8 — shader-flags / texture-role vocabulary). NIFAL canonical-translation finding — see `/audit-nifal`.

**Severity**: MEDIUM — **and it stays MEDIUM.** See the cross-audit measurement below, which settles the escalation condition this finding was originally filed with.
**Tier violated**: `no-leak` — the per-game vocabulary fails to collapse for three of four property kinds, and the wrong game's table silently drops or misroutes an authored override.
**Game Affected**: FO4, FO76, Starfield. Skyrim and earlier are unaffected — their correct layout *is* the default.

**Location**: `crates/nif/src/import/material/dedicated_shader.rs:105` (the only assignment). The missing scene-level assignment belongs at `crates/nif/src/import/material/walker.rs:118`.

## Population — measured by two sibling audits, do not re-investigate

This finding was written with an unmeasured "escalate to HIGH if a corpus probe shows FO4 effect-shader meshes with TXST overrides in shipped cells" condition. Two sibling audits in the same suite measured it:

- **FO4**: `XATO` = **0** and `XTXR` = **0** across every vanilla FO4 master; `XTNM` = 52. Affected population is **35 REFRs**, not thousands. **The escalation condition is not met — this stays MEDIUM.**
- **Starfield**: 927 `BSEffectShaderProperty` blocks across 416 effect-only NIFs, but **zero runtime impact** — the only consumer is the REFR overlay, and Starfield authors no `XATO` / `XTXR` / `XMSP` / `XCWT` at all.

## ⚠️ Ordering dependency — this one-liner must land BEFORE #973

Fixing **#973** (the FO4 MSWP material-swap table, which currently has **zero consumers**) would raise this finding's affected population from 35 REFRs to **~18,000 REFRs**. The MSWP swap path routes through the same slot vocabulary, so wiring that consumer up while `texture_slot_layout` still defaults to `Skyrim` on effect/sky/water meshes converts a 35-REFR curiosity into an 18,000-REFR misroute.

**This one-line fix must land before #973's consumer is wired up.** If #973 is picked up first, fix this in the same PR.

## Description

`86c41022` correctly made slot->role resolution game-aware by adding `TextureSlotLayout` and threading it through `TextureSlotContext`. The layout itself is a pure function of the file's generation — `TextureSlotLayout::from_bsver(scene.bsver)` (`crates/nif/src/import/material/slot_role.rs:102`) — but it is written into `MaterialInfo` at exactly **one** place: inside the `if let Some(shader) = scene.get_as::<BSLightingShaderProperty>(idx)` body.

`apply_dedicated_shader_property` dispatches to **four** property handlers (`apply_bs_lighting_shader`, `apply_bs_effect_shader`, `apply_bs_sky_shader`, `apply_bs_water_shader`); the other three never set it, and neither does the legacy `NiProperty` chain. A mesh with no `BSLightingShaderProperty` therefore keeps `TextureSlotLayout::default()`, which is **`Skyrim`** (`crates/nif/src/import/material/slot_role.rs:91-94`). `crates/nif/src/import/material/mod.rs:1465` then copies that wrong value onto `ImportedMaterial`, and `byroredux/src/cell_loader/spawn/mesh_instance.rs:116` feeds it straight into the `TextureSlotContext` that gates every REFR override.

## Evidence

```
$ grep -n "texture_slot_layout" crates/nif/src/import/material/*.rs
dedicated_shader.rs:105:        info.texture_slot_layout = slot_layout;   <- the ONLY write
mod.rs:479:    pub texture_slot_layout: TextureSlotLayout,
mod.rs:1099:            texture_slot_layout: TextureSlotLayout::default(),   <- = Skyrim
mod.rs:1465:            texture_slot_layout: self.texture_slot_layout,
```

Consequences for an FO4 mesh whose only property is a `BSEffectShaderProperty`, carrying a REFR texture override:

- **slot 2** — correct arm is `(Fallout4, 2) => Some(Emissive)` unconditionally; the Skyrim arm requires `context.glow_map`, which is also only ever set at `dedicated_shader.rs:106`, so it is `false` -> `None` -> **the override is dropped**, not misrouted.
- **slot 3** — correct arm is `(Fallout4, 3) => Some(GreyscaleLut)`; the Skyrim arm yields `Height`, so an FO4 palette gradient is bound as a POM height field and `triangle.frag`'s POM branch (which gates only on `parallaxMapIndex != 0u`) ray-marches over it.
- **slot 5** — correct FO4 arm is `Wrinkle`/`EnvironmentMask` by shader family; the Skyrim arm gates on `tint_family`.
- **slot 7** — correct arm is `(Fallout4, 7) => Some(Specular)` unconditionally (that is the whole point of #2998); the Skyrim arm additionally requires `model_space_normals`, so a specular override on an FO4 effect mesh without the almost-never-set MSN flag is **dropped**.

The *import* side is unaffected — `slot_to_role` is only called from inside `apply_bs_lighting_shader`, which has the correct local `slot_layout` in scope. The defect is confined to the value that **leaves the crate**.

## Impact

Bounded today at ~35 FO4 REFRs (see the measurement above), zero on Starfield. The reason this is MEDIUM rather than LOW is **structural, not statistical**: a per-game discriminator that silently defaults to a *different real game* is the exact failure mode #2695 was filed for, and the `record_unrouted_texture_slot` counter added in the same commit **cannot see it** — the wrong-table lookups either succeed with the wrong role (invisible) or return `None` for a reason the counter attributes to the wrong layout bucket.

And the population is one wiring change (#973) away from ~18,000 REFRs.

## Suggested Fix

One line at `crates/nif/src/import/material/walker.rs:118`, immediately after `let mut info = MaterialInfo::default();`:

```rust
info.texture_slot_layout = TextureSlotLayout::from_bsver(scene.bsver);
```

The layout is a property of the *scene*, not of any one property block, and that function already takes `scene`. Leave the assignment in `apply_bs_lighting_shader` (harmless — it recomputes the same value), or drop it.

Pin with a test asserting an FO4-bsver mesh carrying only a `BSEffectShaderProperty` reports `TextureSlotLayout::Fallout4`.

## Related

- #2695 — the two-disagreeing-tables defect this table was created to fix.
- #2998 / #3085 / #2999 — the per-game arms that make the layout load-bearing.
- #2697 (OPEN) — the third hand-written role walk, same "unprotected parallel structure" family.
- **#973** — the FO4 MSWP swap table. See the ordering dependency above.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: the layout is established once at the scene boundary (`walker.rs`), not re-derived per property block or at render time. See `/audit-nifal`.
- [ ] **SIBLING**: all four property handlers (`apply_bs_lighting_shader` / `_effect_` / `_sky_` / `_water_`) **and** the legacy `NiProperty` chain end up carrying the correct layout
- [ ] **ORDERING**: confirmed landed before (or with) #973's MSWP consumer — otherwise the population jumps to ~18,000 REFRs
- [ ] **TESTS**: an FO4-bsver mesh with only a `BSEffectShaderProperty` asserts `TextureSlotLayout::Fallout4`
