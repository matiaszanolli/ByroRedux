# #2552: FO3-D5-NEW-04: spawn_collision_shapes's catch_unwind guards a Clone that can't panic; stale comment

**Severity**: LOW
**Dimension**: FO3 Collision Import (Havok → CollisionShape)
**Location**: `byroredux/src/cell_loader/spawn.rs:956-965`
**Status**: NEW
**Labels**: documentation, import-pipeline, low, tech-debt

## Description
`spawn_collision_shapes`'s `catch_unwind` wraps `coll.shape.clone()` with the comment "parry3d panics on nested Compound shapes. Clone inside catch_unwind so a bad shape doesn't kill the entire load." But `coll.shape` is a canonical `CollisionShape` enum — a plain Rust data structure — and `.clone()` on it cannot panic regardless of shape nesting; it's a pure data copy. `#373` restructured the physics conversion (`crates/physics/src/convert.rs`) to depth-first-flatten any Compound-of-Compound into a `Vec<(Isometry3, SharedShape)>` specifically so parry3d/Rapier never sees a nested `SharedShape::compound` — the panic condition this comment describes was the *old*, pre-#373 conversion shape, not anything `Clone` could ever trigger.

## Evidence
Confirmed directly: `spawn.rs:956-965` — the comment describes a parry3d panic risk, but the guarded expression is `std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| coll.shape.clone()))`, a `Clone` call with no parry3d/Rapier involvement. `convert.rs`'s own module doc confirms the flattening approach: "Parry / Rapier forbid composite-inside-compound... Returning a `Vec<(Isometry3, SharedShape)>` instead of a single `SharedShape::compound`... See #373."

## Impact
None on running code — the `catch_unwind` is harmless dead-weight (guards a call that structurally cannot panic) and the warning log path is unreachable. Hygiene/documentation issue: a future reader trying to understand the panic risk will chase a mechanism (nested-Compound parry3d panic) that no longer exists at this call site.

## Related
`#373` (the flattening fix that removed the actual constraint this comment describes).

## Suggested Fix
Remove the now-unnecessary `catch_unwind`/`AssertUnwindSafe` wrapper around the plain `.clone()` call, or if retained defensively, correct the comment to state it's a legacy guard with no known trigger post-#373.

## Completeness Checks
- [ ] **TESTS**: If the `catch_unwind` is removed, confirm no test relies on its warning-log fallback path

---

# #2555: FNV-D2-02: classify_pbr_keyword's env-map arm is documented as the FNV majority path but is unreachable on ~83% of sampled FNV meshes post-#2315

**Severity**: MEDIUM
**Dimension**: NIFAL Canonical Translation (FNV slice)
**Location**: `crates/core/src/ecs/components/material.rs:681-727`
**Status**: NEW
**Labels**: documentation, renderer, medium

## Description
`classify_pbr_keyword`'s env-map arm's in-source comment claims `env_map_scale = 1.0` is FNV's neutral default on "nearly every FNV surface." That premise was invalidated by #2315 (CLOSED), which forces `env_map_scale` to 0.0 unless an explicit environment-mapping shader flag is authored. Measured: `env = 0.00` on 15 of 18 sampled FNV meshes. Compounding it, the arm's metalness lift reads `spec_lum`, which is 0.0 on all 18 sampled meshes per FNV-D2-01 (this session) — so even meshes that do reach the arm cannot produce `metalness > 0` from it. Not a wrong output on its own (matte fallback is defensible), but the single PBR decision point for all legacy content now documents a false reachability story, risking future audits/fixes reasoning from a stale premise.

## Evidence
Confirmed directly at `material.rs:680-695`: the comment reads "`BSShaderPPLighting` authors `env_map_scale = 1.0` as the neutral default on nearly every FNV surface, so this arm catches the vast majority of interior content."

## Impact
Not a wrong output on its own. Risk is to future audits/fixes reasoning from the stale reachability claim.

## Related
#2315 (CLOSED), #1873, #2352, FNV-D2-01 (this session).

## Suggested Fix
Correct the comment to state post-#2315 reachability; decide explicitly whether to retire or re-source the specular-luminance conductor lift. If FNV-D2-01 is fixed as suggested, this arm stays correctly inert and only the comment needs updating.

## Completeness Checks
- [ ] **TESTS**: N/A unless the arm's logic is also changed; if only the comment is corrected, no test needed

---

# #2556: FNV-D2-03: EmissiveSource::None's doc contradicts Material::default()

**Severity**: LOW
**Dimension**: NIFAL Canonical Translation (FNV slice)
**Location**: `crates/core/src/ecs/components/material.rs:453-458` vs `:359-362`
**Status**: NEW
**Labels**: documentation, renderer, low

## Description
`EmissiveSource::None`'s variant doc says `emissive_mult` defaults to 0.0, but `Material::default()` sets it to 1.0. No production impact on the NIF path (translation always overwrites it from `ImportedMaterial`, whose own default is 0.0); bites only direct `Material::default()` call sites (`cornell.rs`, save/load fixtures).

## Evidence
Confirmed directly: `Material::default()` (`material.rs:359-362`) sets `emissive_mult: 1.0`; `EmissiveSource::None`'s doc comment (`material.rs:453-458`) says "No emissive authoring; `emissive_mult` defaulted to 0.0."

## Impact
Documentation-only mismatch; misleading to anyone reading the enum doc as authoritative for `Material::default()`'s actual field values.

## Suggested Fix
Either change `Material::default()`'s `emissive_mult` to 0.0 (verify no call site depends on 1.0), or correct the `EmissiveSource::None` doc comment to say 1.0.

## Completeness Checks
- [ ] **TESTS**: If `Material::default()`'s value changes, confirm no `cornell.rs`/save-fixture call site depends on the old 1.0 default

---

# #2557: FNV-D4-01: feature-matrix.md mislabels its SCPT record count as FO3/FNV -- that figure is FO3-only

**Severity**: LOW
**Dimension**: ESM Record Parser
**Location**: `docs/feature-matrix.md:152`
**Status**: NEW
**Labels**: documentation, import-pipeline, low

## Description
`docs/feature-matrix.md:152` mislabels its SCPT record count ("1,257") as "FO3/FNV" — that figure is FO3-only; real `FalloutNV.esm` ships 2,576 SCPT records (parser correctly captures all of them; no functional gap).

## Evidence
Confirmed directly: `feature-matrix.md:152` reads "ESM SCPT record parse (FO3/FNV, 1 257 records)".

## Impact
Documentation only. The parser itself is correct and unaffected.

## Suggested Fix
Split the row into separate FO3 (1,257) and FNV (2,576) counts, or clarify the figure is FO3-specific.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
