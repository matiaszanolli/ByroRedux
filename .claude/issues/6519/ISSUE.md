# SF-2026-10-27-D4-01: 1efe39722's VFX-collider gate keys on `ImportedMaterial.effect_shader`, which a Starfield `BSEffectShaderProperty` material-reference stub never fills - 88 stubs in 66 NIFs keep a synthesized collider

Labels: low, game:starfield, legacy-compat, physics, bug
Source: docs/audits/AUDIT_STARFIELD_2026-10-27.md

**Source**: `docs/audits/AUDIT_STARFIELD_2026-10-27.md` finding `SF-2026-10-27-D4-01` (LOW)
**Location (re-mapped to current tree)**: `byroredux/src/cell_loader/spawn.rs` (`render_geometry_can_collide`, ~:83-90); `crates/nif/src/import/material/dedicated_shader.rs` (`apply_bs_effect_shader`: the `shader.material_reference` arm does `info.material_kind = 101; return;` before `info.effect_shader = Some(effect)`); `byroredux/src/cell_loader/spawn/mesh_instance.rs` (~:1525-1531, `ArchitectureTriMesh` call site)

**Severity**: LOW. Dimension: cell bring-up / synthesized colliders (physics side belongs to `/audit-physics`). Game: Starfield. Partial of 1efe39722 (2026-10-26, no issue number). Related to FNV-2026-10-27-02, which hits the same predicate through a different block (`BSShaderNoLightingProperty`); fix the two together.

## Description
- 1efe39722 made the missing-collision fallback skip VFX sheets by requiring `material.effect_shader.is_none()` (plus `!is_decal`, `!alpha_test`, `material_kind != MATERIAL_KIND_FIRE_REFRACTION`).
- On Starfield, a `BSEffectShaderProperty` with a non-empty name is parsed as a material-reference stub (`effect.rs`). `apply_bs_effect_shader` then returns early after tagging `material_kind = 101`, so `effect_shader` stays `None`. The early return is deliberate (#2617): the stub's fields are placeholders.
- The FO76 analogue is repaired downstream because a resolved `.bgem` inserts `effect_shader` (`merge.rs` ~:1797-1809). Starfield ships zero `.bgem` files and its CDB arms never insert one.
- Every Starfield static collider is synthesized from render geometry (collision is an opaque `BhkSystemBinary`), so these shapes still become `Fixed` trimesh colliders.
- The 783 full-body Starfield effect blocks *are* excluded now; the rule depends on which of the two encodings a mesh uses.

## Evidence (scratch probe over `Starfield - Meshes01/02/MeshesPatch.ba2` + `ShatteredSpace - Main01.ba2`)
- 871 `BSEffectShaderProperty` blocks: 783 full-body and **88 stubs in 66 NIFs**.
- The full-body blocks are almost all editor geometry (`meshes\markers\...` 392, gizmos, `marker_*.nif`, empty-name helpers).
- The stubs sit in content: architecture 24, setdressing 33, ships 15, starstations 6, furniture 6, animobjects 2, actors 1, markers 1. Names: `newatlantis\glowwhite.bgem` 12, `sciextledspot.bgem` 11, `opiextlightglow03.bgem` 8, `minerss_helmglass.bgem` 8, `glassfrostedpatterntri01.bgem` 6, ... Example NIFs: `sciintrmsmwallmid_plugc_r00.nif`, `shpgenintaddon_glasspane_opaque01.nif`, `floodlamp_tripodlights_on01.nif`, `neondrink01.nif`.
- `material_kind` is never rewritten after import; its only writers are `dedicated_shader.rs` (4 sites) and `legacy_properties.rs` (2), so the stub's 101 reaches the gate intact.

## Impact
Glow strips, LED spots, console screens and poster glow cards keep invisible colliders. Most are flush with the wall they decorate, so impact is small (LOW, unlike FNV-02's free-standing beams). ~15 of the 88 are glass panes where a collider is arguably correct; today that is accidental, not decided. Starfield's real VFX (987 VFX-named NIFs, 883 `BSLightingShaderProperty` `.mat`-only) carry effect-ness only in the CDB `EffectSettingsComponent` blend mode; no import-side gate sees it until #5509 forwards the additive family (cited, not re-filed).

## Related
FNV-2026-10-27-02, #5509, #2617, 1efe39722.

## Suggested Fix
Key `render_geometry_can_collide` on effect-shader provenance that survives the stub arm (`material_kind == MATERIAL_KIND_EFFECT_SHADER`, or a provenance bit set by the stub arm). Decide glass panes explicitly (keep a collider for glass-named / `glass_enabled` materials if intended). Add a stub case (`effect_shader: None`, `material_kind: 101`) to `effect_shader_sheets_do_not_synthesize_collision` (`cell_loader/spawn/synthesize_trimesh_tests.rs`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling systems
- [ ] **CANONICAL-BOUNDARY**: Stub provenance is carried through the NIFAL material boundary (`ImportedMaterial`), not re-derived at spawn time
- [ ] **TESTS**: A regression test pins this specific fix

