# FNV-D2-02

**Issue**: #3335
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 2 — NIFAL Canonical Translation
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/core/src/ecs/components/material.rs:729`, `:778`, `:807`

**Premise verified**: same `contains_any_ci` whole-path matcher; `iron`, `log`
and `face` are all short enough to be substrings of common English/asset words,
exactly the collision class #2009 fixed for `ice`/`gem`/`fur`.

**Evidence** (`Fallout - Meshes.bsa`, live import):

```
== metal/iron: 27 meshes, 4 distinct paths
     23 textures\effects\neongreenironsights.dds     -> metalness 0.90 / rough 0.55
      1 textures\effects\windowenvironmentmap01.dds  -> "envIRONment"; a WINDOW classified as metal
      2 textures\weapons\2handmelee\9iron.dds        (correct — a golf iron)
      1 textures\weapons\1handmelee\tireiron.dds     (correct)

== wood/log: 64 meshes, 22 distinct paths     -> roughness 0.70 (wood)
     12 textures\terminals\nv_blackjack\nv_blackjack-casino-logo03.dds
      6 textures\landscape\plants\nv_buffalogourd_d.dds   ("buffaLO Gourd")
      5 textures\armor\eulogyjones\male_d.dds             ("euLOGy")
      4 textures\clutter\casino\nvslotmachinetopslogo_d.dds

== skin/face: 107 meshes, 52 distinct paths   -> roughness 0.50 (skin)
     17 textures\interface\hud\air_meter.dds          ("interFACE")
     17 textures\terminals\xbox\english\racesexinterface01.dds
     16 textures\interface\hud\stealth_indicator.dds

== skin/head: 255 meshes  -> 41 bobblehead\vaultboy, 14 sky\wasteland_overhead,
                             6 armor\headgear\legionhelmet06 (a metal helmet), …
```

**Impact**: ~450 further FNV meshes carry a fabricated material class —
`windowenvironmentmap01` shading as 0.9-metalness steel is the worst single
case (it crosses `metalness > 0.3`, so it *does* fire an RT environment
reflection ray it never earned). Bounded population; cosmetic.

**Fix sketch**: same as FNV-D2-01 — `contains_any_ci_word` for `iron`, `log`,
`face`, `head`, `body`, and consider scoping `iron` to `filename` alongside
`dwemer`/`dwarven`.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
