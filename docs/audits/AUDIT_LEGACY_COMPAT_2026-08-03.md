# Legacy Compatibility Audit — 2026-08-03

**HEAD:** 1ae86f62 · **Type:** legacy-compat (canonical-translation boundary pass) ·
**Run as:** one leg of the `comprehensive` audit-suite sweep

**Scope:** Compatibility/mapping gaps between Gamebryo 2.3 / Creation-engine
behaviour and Redux, framed by the three canonical translation layers
(NIFAL / EXAL / PHYSAL) plus coordinate-system correctness, the per-game
translation survey, and subsystem coverage vs. the legacy headers (all 7
dimensions of `.claude/commands/audit-legacy-compat/SKILL.md`).

**Method:** Delta pass over the 2026-07-25 report (base ca7a4e0e → HEAD
1ae86f62, 122 commits). Read the prior report in full as the leak-inventory
baseline, then walked the 122-commit `git log` looking for anything
touching a translate boundary, a coordinate swap, a per-game branch, or a
new content-source format. Ran the mandatory dedup step (`gh issue list` →
47 currently-open issues saved to `/tmp/audit/issues.json`) and individually
re-checked the ten still-open items carried over from 07-25 plus the two
that report listed as fixed-in-flight. Every candidate finding below was
traced to its call sites and read in full before being accepted or
rejected.

**Headline: zero new findings.** The 122-commit window was dominated by
renderer work (volumetric fog, shadow-policy refactor, FSR tuning),
Scaleform/UI (AVM1/AVM2 host bridge), and the M47.2 scripting/cinematic
slice (MQ101 quest, `hkx` Havok packfile reader) — none of it compat-surface
work in the NIFAL/EXAL/PHYSAL sense. The handful of commits that *did* touch
a translation boundary (Oblivion packed-collision winding, NIF light
kind/direction/cone, `NiBillboardNode` propagation, light shadow-flag
canonicalization, WRLD `NAM3`/`NAM4`/`OFST` parsing, the FO3 water-shader
premise) are all genuine, already-tracked bug fixes — each re-verified in
place below — not new leaks, and none reintroduced a duplicated coordinate
swap or a per-game branch downstream of a boundary.

---

## Boundary verification results (no findings — recorded for the trail)

| Layer | Claim verified | Result |
|---|---|---|
| **NIFAL — material** | `translate_material` sole populated-`Material` producer, post texture-role-unification refactor (`1d94eb24`, `05d68926`, `c8c8a834`) | **Clean.** This window's big material refactor (`MaterialTextureSet<T>`'s 18 named roles replacing per-game slot numbers) postdates the 07-25 report and had not been individually re-verified until now. Grepped every non-test call site of `translate_material(` — still exactly two production callers (`byroredux/src/cell_loader/spawn.rs:1303`, `byroredux/src/scene/nif_loader.rs:879`). `crates/nif/src/import/material/mod.rs` carries cross-era bit-equivalence as compile-time `const _: () = assert!(...)` checks (e.g. `fo4_slsf1::DECAL == skyrim_slsf1::DECAL == fo3nv_f1::DECAL`), which is the correct shape for Pattern A (named constant, not a duplicated per-game literal). No dropped role: all 18 `MaterialTextureSet` fields are populated identically regardless of source format via `map_ref`. |
| **NIFAL — collision winding** | Packed-mesh (`bhkPackedNiTriStripsShape`) and strip (`NiTriStripsData`) de-stitchers stay single-boundary | **Clean, and improved.** `a4c11bfb` (#2193 phase 1) deduplicated a second hand-written strip-parity copy that had silently drifted to a cyclically-equivalent-but-independent implementation; `c4481c78` (#2193 phase 2) added `packed_triangle_winding()` — a single new function, one call site (`crates/nif/src/import/collision/shape.rs:434`) — to correct Oblivion-only authored `TriangleData.Normal` mismatches. Both stay inside the existing single-producer contract; the CCW last-two-vertex swap convention (Dimension 1) is unchanged. |
| **NIFAL — lights** | `imported_light_from_base` sole `LightSource.kind/direction/outer_angle` producer | **Clean.** #2205 (`1a6296e2`) added the three fields to canonical `LightSource` and wired them from the single import-side function (`crates/nif/src/import/walk/mod.rs:1370`) through `spawn_nif_lights` (`byroredux/src/cell_loader/spawn.rs:521`). Every other `LightSource { … }` construction site outside these two is inside a `#[cfg(test)]` module (verified by reading the enclosing `mod` for each of the 5 hits in `render/lights.rs`) or save-deserialization (`save_io.rs`). Fixes a real, measured content gap: 95 `NiDirectionalLight` blocks in Oblivion's `Meshes.bsa` previously rendered as full-bright omni point lights. |
| **NIFAL — `NiBillboardNode`** | `extract_billboard_mode` sole producer, consumed by both hierarchical and flat walks | **Clean.** #2206 (`4fd214aa`) added `ImportedMesh::billboard_mode` as the flat-walk sibling of the existing `ImportedNode::billboard_mode`, both fed by the same `extract_billboard_mode()` (`crates/nif/src/import/walk/mod.rs:1695`). Fixes a real, measured gap: 213–1,527 `NiBillboardNode` instances per vanilla archive never rotated to face the camera on the cell-loader (one-entity-per-mesh) path. |
| **EXAL — shadow-flag canonicalization** | New `canonical_light_shadow_flags(game, source_flags)` follows the existing `canonical_light_animation_flags` GameVariant-table shape | **Clean — correct Pattern B shape.** #2250 (`01f198e7`) fixed a real leak: `render/lights.rs` was reading raw `LIGH.DATA` flags and unconditionally applying Skyrim's TES5 bit layout to every `GameKind` with no boundary function. The fix moves canonicalization to spawn time (`cell_loader/references/mod.rs:1116,1120,1164,1168`), mirrors the sibling function's per-game match shape, and — verified by grep — leaves **zero** `GameKind::` / `game ==` hits anywhere in `crates/renderer/src` or `byroredux/src/render`. |
| **EXAL — WRLD `NAM3`/`NAM4`/`OFST`** | Previously-open #1849 (parse gap) | **Fixed, verified in place; consumer gap remains and is already documented.** `560c6741` lands `lod_water_form` / `lod_water_height` / `cell_offsets` on `WorldspaceRecord` (`crates/plugin/src/esm/cell/mod.rs:854,860,873`), with era-branching tests. Grepped for a consumer across `byroredux/src` — zero hits. This is the exact "parsed but not yet consumed" state the skill's own EXAL section already documents (§5.4) and #1849's own fix scope explicitly excluded the consumer; not a new finding. |
| **EXAL — FO3 water-shader "dead end"** | Previously-open #1856 | **Disproved premise, now documented and closed.** `595a1898` shows the original finding misread `WaterShaderProperty` as a partial wire-up; per `nif.xml` line 6322 that block inherits `BSShaderProperty` with zero fields of its own — `Water Shader Flags` only exists on Skyrim-era `BSWaterShaderProperty`. Nothing to fix; the issue is closed as documentation. |
| **PHYSAL — ragdoll** | one translate, one build; extract game-agnostic | **Unchanged, still clean.** No commits in this window touch `crates/nif/src/import/collision/ragdoll.rs`, `crates/nif/src/blocks/collision/constraints.rs`, or `byroredux/src/ragdoll.rs`. |
| **New animation format (`hkx`) funnels into the existing canonical sink** | The Session-62 MQ101 `hkx` Havok packfile reader (`crates/hkx/src/`) does not bypass the animation abstraction | **Clean, correct shape.** `byroredux/src/asset_provider/animation.rs::convert_hkx_clip` (line 165) converts decoded Havok spline/static tracks into the *same* canonical `AnimationClip` type (`crates/core/src/animation/types.rs`) used by `anim_convert::convert_nif_clip`, registered in the same `AnimationClipRegistry`. This is the same "N format-specific producers, one canonical sink" pattern already used for geometry (`ni_tri_shape.rs`/`bs_tri_shape.rs`/`bs_geometry.rs` → `ImportedMesh`) — worth recording since it's a brand-new content-source format and a natural place for a parallel pipeline to creep in, but it doesn't. |
| **Scaleform/UI `ScaleformProfile`** | New AVM1/AVM2 host-bridge landing (unowned subsystem, per `_audit-common.md`) doesn't leak a `GameKind` branch | **Clean, correct shape.** `ScaleformProfile::detect` (`crates/ui/src/profile.rs`) discriminates `SkyrimAvm1`/`Fallout4Avm2` from the SWF's own `FileAttributes` (wire-format self-discrimination — the same Pattern B shape the skill calls out as correct), not a `GameKind` check. Zero `GameKind::`/`game ==` hits in `crates/ui/src`. Noted per the un-owned-subsystem coverage rule in `_audit-common.md` — this audit only grepped for a compat-boundary leak, not a full UI review (that's `/audit-safety`'s territory for the Ruffle FFI surface). |

---

## Findings

None. Every changed file touching a translation boundary in this window
was either a verified bug fix (tracked issue, now closed, re-checked in
place) or stayed inside its existing single-producer contract.

---

## Still-open tracked gaps re-verified (Existing — do not re-file)

Re-checked each of the ten items carried from the 07-25 report individually
against the current `gh issue list` snapshot; all ten remain **OPEN** and
unchanged:

- **#1827** (FO4-D4-02) — Starfield `BSGeometry` leaves per-vertex bone
  indices/weights empty. Informational, out of FO4 scope.
- **#1576** (SF-D4-03) — Model-less STAT/BNDS/ACTI/ARMO Starfield forms
  drop (geometry lives in a BFCB component block).
- **#1981** (FNV-D7-02) — Skinned-mesh `WorldBound` does not track a
  ragdoll that leaves its origin.
- **#2098** (SF2D2-01) — `BSGeometry` bounding-sphere scale not
  cross-checked against havok-scaled vertices.
- **#2099** (SF2D2-02) — Secondary UV channel (`uvs1`) parsed then dropped.
- **#2108** (SF-D9-01) — `EFFECT_PALETTE_COLOR/ALPHA` derived from
  LUT-texture presence, not the authored palette-enable flag.
- **#2109** (SF-D9-02) — BGEM v21/v22 glass-overlay params +
  envmap-mask-scale + v11 emittance dropped in merge.
- **#1822** (SPT-NEW-07) — `MaybeStringElseBare` (tag 13005) misparse risk.
- **#1843** (NIF-D1-01) — Pre-4.1 NIF bool fields read as 1 byte where the
  wire format is 32-bit on Morrowind-era NIFs.

Two items from the 07-25 carry-forward list have since **closed** and are
recorded fixed above rather than re-listed here: **#1849** (WRLD
`NAM3`/`NAM4`/`OFST` parse) and **#1856** (FO3 water-shader premise).

## Documented limitations re-confirmed (NOT findings — do not re-file)

- **FO4/FO76/Starfield ragdolls** — blocked on
  `BhkNPCollisionObject → BhkSystemBinary` decoder. Unaffected this window.
- **`NiFogProperty`** — intentionally not dispatched (#1224).
- **Emissive scale** — three `EmissiveSource` variants share ~1.0 scale; no
  normalization is correct.
- **Sun latitude** — no authored CLMT/WRLD latitude field exists.
- **VWD full-model cull** (#1889, closed) — the per-record `VisibleWhenDistant`
  marker is now materialized at spawn (`stamp_visible_when_distant`,
  `byroredux/src/cell_loader/references/mod.rs:1401`); the *active* cull that
  would read it is a deliberately deferred design decision documented
  in-line at `byroredux/src/cell_loader/object_lod.rs:328-332` ("deferred...
  once the full-detail radius is decoupled from the ring"), not an
  untracked gap.
- **M42 AI-package v0 scope** — unchanged.
- **`PACK` `PSDT` era gap** — fixed prior window, unchanged.

---

## Summary

- **Total findings**: 0
- **CRITICAL**: 0
- **HIGH**: 0
- **MEDIUM**: 0
- **LOW**: 0

The compat surface stays clean at 1ae86f62. All boundaries re-verified this
pass — material (post texture-role-unification refactor), collision
winding, lights (kind/direction/cone + shadow-flag canonicalization),
`NiBillboardNode`, WRLD LOD-water parsing, ragdoll, and the brand-new `hkx`
animation format's single-sink integration — are single-producer clean with
no per-game branch downstream of a translate boundary. Six commits in this
window were genuine legacy-compat fixes (#2193 ×2, #2205, #2206, #2250,
#1849, #1856); all six re-verified fixed in place. Nine pre-existing tracked
issues (mostly Starfield/SpeedTree/NIF-legacy scoped, owned by the relevant
per-game/NIF audits) remain open and unchanged; nothing new needs filing
from this leg of the sweep.

---

*No `/audit-publish` step is needed — zero findings to file.*
