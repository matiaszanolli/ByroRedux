# FNV Compatibility Audit — 2026-07-15 (Dimension 1: Cell Loading End-to-End)

Scope: `--focus 1` — single-dimension run of `/audit-fnv`. Covers interior
load (Prospector Saloon), exterior 7×7 grid (WastelandNV), the
`NifImportRegistry` Arc cache, cell-unload hygiene regression guards
(#1520/#1531), M38 water/submersion, and `_far.nif` distant-object LOD
(#1726/#1745). Verified against the tree as of 2026-07-15.

## Executive Summary

Dimension 1 remains healthy. A prior FNV audit
([`AUDIT_FNV_2026-07-13.md`](AUDIT_FNV_2026-07-13.md), 2 days old) already
scored this exact dimension "Clean"; `git log` confirms zero commits touched
any file in this dimension's scope since then, so this pass independently
re-traced the same code paths (not just trusted the prior doc) and confirms
every one of its regression guards is still intact — including a **live
re-run** of the #1520/#1531 cell-unload regression suite (`cargo test --bin
byroredux release` → 19/19 passed).

One **NEW MEDIUM** finding surfaced in a case the prior audit didn't examine:
`resolve_cell_lighting`'s own doc comment promises a 3-way fallback (XCLL →
LTMP → "engine default"), but the third arm was never implemented — when an
interior cell authors neither, all four production call sites silently skip
applying any lighting at all, leaving whatever `CellLightingRes` a *previous*
cell installed. This reproduces the exact #1340/#1282 stale-lighting failure
class (wrong ambient/fog, exterior sun leaking into a sealed interior) for
that one input shape. It is **not a regression** of #1340/#1282 (their fixed
`Some` path is confirmed intact) — it's an uncovered edge case in the same
fallback chain. Practical FNV blast radius is low (FNV predates LTMP and
vanilla FNV interiors, including Prospector Saloon, always author `XCLL`
directly) but the gap is real in the shared, game-agnostic loader and is
reachable by any interior cell — any game — with a missing/unresolvable LTMP
master.

No live bench was run this pass (static/functional trace + targeted
`cargo test`, per the checklist's actual scope); no bench-of-record
comparison table follows. ROADMAP's Prospector Saloon bench-of-record
(76.2 FPS / 13.11 ms / fence=11.12 ms / 3516 ent / 1224 draws @ `1c26bc25`,
2026-06-03) is flagged 613 commits stale as of Session 56 and was not
refreshed by this run.

## Dimension Findings

### FNV-D1-01: Interior cell lighting resolving to `None` (no XCLL, no resolvable LTMP) leaves `CellLightingRes` un-updated
- **Severity**: MEDIUM
- **Dimension**: Cell Loading End-to-End — Interior Lighting Resolution
- **Location**: `byroredux/src/cell_loader/load.rs:473-511` (`resolve_cell_lighting`); call sites at `byroredux/src/scene.rs:214-219`, `byroredux/src/cell_loader/transition.rs:264-269`, `byroredux/src/debug_load.rs:247-251`, `byroredux/src/save_io.rs:657-659`
- **Status**: NEW (not a regression of #1340/#1282 — their `Some(lit)` fix is intact; this is an uncovered edge case in the same fallback chain those fixes' own doc comment enumerates as case 3)
- **Description**: `resolve_cell_lighting`'s doc comment states a 3-way contract: (1) explicit XCLL wins, (2) an LTMP template synthesizes `CellLighting` when XCLL is absent, (3) "no XCLL and no resolvable LTMP → `None` (engine default)". In practice there is no "engine default" applied anywhere: `load_cell_with_masters` returns `CellLoadResult { lighting: None, .. }`, and every one of the four production call sites gates the apply behind `if let Some(ref lit) = result.lighting { apply_interior_cell_lighting(world, lit); }`. When `lighting` is `None`, that call — and therefore `world.insert_resource(CellLightingRes::from_cell_lighting(...))` — never runs, leaving the resource exactly as the previous cell left it.
- **Evidence**:
  ```rust
  // load.rs:472 doc comment vs. actual behavior
  /// 3. **No XCLL and no resolvable LGTM** → `None` (engine default).
  pub(crate) fn resolve_cell_lighting(...) -> Option<esm::cell::CellLighting> {
      if let Some(lit) = cell.lighting.clone() { return Some(lit); }
      let template_form = cell.lighting_template_form?;
      let template = index.lighting_templates.get(&template_form)?;
      Some(esm::cell::CellLighting { ... })
  }
  ```
  ```rust
  // transition.rs:264-269 — same guard shape at all 4 call sites
  // #1340 — apply the loaded interior's lighting ... Without it the door-walked
  // interior keeps the previous cell's `CellLightingRes`.
  if let Some(ref lit) = result.lighting {
      super::apply_interior_cell_lighting(world, lit);
  }
  ```
  `render/lights.rs:103-124` (`collect_lights`) reads whichever `CellLightingRes` is currently installed — including its `is_interior` flag — with no separate "nothing resolved" branch, so a stale resource from a *previous* (possibly exterior) cell is used verbatim.
- **Impact**: For an interior cell authoring no `XCLL` whose `LTMP` FormID either isn't set or fails to resolve in `index.lighting_templates` (missing/renamed master, corrupt plugin, or any interior authored without either field), the previous cell's `CellLightingRes` — potentially an exterior cell's with `is_interior: false` and full-strength directional sun — silently carries into the new interior: the exact #1282/#1340 failure mode. On the very first cell load in a session (no prior resource at all), the effect is milder — `try_resource` returns `None` in `collect_lights`, producing an unlit interior rather than a mislit one. Practical FNV risk is low: FNV predates LTMP/LGTM and vanilla FNV interiors (including Prospector Saloon) always author XCLL directly. The gap is reachable on any game sharing this loader — e.g. a Skyrim interior whose LTMP FormID fails to resolve because a required master is missing from the load order, or a broken mod plugin on any supported game.
- **Related**: Same code family as the already-fixed #1340 / #1282 (their `Some` path verified intact this pass); not a regression of either.
- **Suggested Fix**: When `resolve_cell_lighting` returns `None`, have the four call sites apply an explicit "engine default" `CellLightingRes` (`is_interior: true`, neutral ambient, no directional sun) instead of skipping the call — e.g. widen `apply_interior_cell_lighting` (or add a sibling) to accept `Option<&CellLighting>` and install the hardcoded default on `None`, so an interior cell load can never inherit a stale or exterior-sourced lighting resource.

## Regression Guards Verified Intact (not re-proposed)

- **XCLL → LTMP → `apply_interior_cell_lighting` `Some` path** (`load.rs:58-76,473-511`) — single shared helper called from all 4 entry points (startup `--cell`, door-walk transition, `cell.load` debug command, M45.1 live-load), preventing the pre-#1340 drift where only the startup path applied lighting. Confirmed correct for every case where `resolve_cell_lighting` returns `Some`; the one gap in the `None` arm is FNV-D1-01 above.
- **`NiAlphaProperty` → decal routing** (`crates/nif/src/import/material/walker.rs:103-156,606-622`, `.../mod.rs:1084-1108`, `byroredux/src/cell_loader/spawn.rs:1009-1058`) — FNV's legacy alpha/decal chain walked once per shape; `apply_alpha_flags` prioritizes alpha-test over blend and marks consumption even for an explicit opaque (`flags=0`) property so a parent `NiNode`'s property can't silently override a shape's own choice (#1201/#982). `is_decal_from_legacy_shader_flags` correctly uses the FO3/FNV-specific flags2 bit 21, verified not reused with a different meaning on Skyrim+/FO4 (#414). Decal escalation to `RenderLayer::Decal` runs after small-STAT-to-Clutter escalation so coplanar decals win z-fight. Collider synthesis correctly excludes decal/alpha-tested geometry (`spawn.rs:1117-1118`).
- **`NifImportRegistry` Arc cache** (`nif_import_registry.rs`, `references/mod.rs:596-736,822-881`) — genuine process-lifetime, three-tier lookup that parses each unique model path exactly once across the whole process, including across exterior-grid cell-boundary crossings. LRU eviction (`BYRO_NIF_CACHE_MAX`, default 2048) frees `AnimationClipRegistry` clip handles in lockstep with the #1854 fix; no re-introduction of the historical "evicted key's clip handle never released" bug.
- **Cell-unload hygiene / #1520 & #1531** (`unload.rs`) — BLAS dropped per freed mesh handle with correct refcount-aware sharing; texture refcounts released for all 6 texture-bearing component types + terrain-tile-slot layer refs (#627); `release_victim_item_instances` (#896) and `release_victim_rapier_bodies` (#1520, extended to `Ragdoll` for #1531) both run before despawn. **Live-verified**: `cargo test --bin byroredux release` → 19/19 passed (`rapier_release_tests`, `inventory_release_tests`, `unload_skin_cleanup_tests`, `sky_params_cleanup_tests`).
- **M38 water plane + submersion** (`water.rs`, `systems/water.rs`) — plane height/extent from `XCLW`/`XCWT` with sane interior defaults; `submersion_system` does full 3-D AABB containment with a hysteresis band (#1450) preventing waterline strobing; `NormalMapHandle` reachable by the unload walk (#1338).
- **`_far.nif` distant-object LOD** (`placement_lod.rs`) — Oblivion/FO3/FNV `DistantLOD` SoA placement + `far_nif_path` derivation correct; streaming gated on `radius_unload` (not `radius_load`, avoiding the #1866/#1871 one-cell-early z-fight); mutually exclusive with the Skyrim+/FO4 `.bto` scheme via two independent no-op-on-wrong-game gates in `app_step.rs:187-203`.

---
Suggest: `/audit-publish docs/audits/AUDIT_FNV_2026-07-15_DIM1.md`
