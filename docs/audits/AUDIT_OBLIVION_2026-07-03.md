# Oblivion (TES4) Compatibility Audit — 2026-07-03

**Scope**: ByroRedux readiness for *The Elder Scrolls IV: Oblivion* content — NIF
v20.0.0.5 retail + the v10.x NetImmerse tail (both sizeless), BSA v103, the live
ESM path, Oblivion legacy shaders, NIFAL canonical material translation, and the
exterior blocker chain.

**Method**: All 7 dimensions executed against HEAD `8498e559`, as part of a
21-audit comprehensive sweep. Each checklist item re-read against current
source; every claim independently confirmed at file:line; `cargo test
-p byroredux-nif` and full `cargo test --workspace` re-run; `nif_stats` re-run
live against vanilla `Oblivion - Meshes.bsa`. Findings attempted-to-disprove
before inclusion.

**Data availability**: `Oblivion/Data/` present. Real-data validation ran for
Dimensions 2, 3, and 6.

**Dedup baseline**: `gh issue list` (71 issues, all states, 2026-07-03) scanned
via `/tmp/audit/issues.json` — no open issue covers any Oblivion NIF/BSA/ESM/
render finding from this sweep. Prior reports in `docs/audits/` scanned,
including yesterday's `AUDIT_OBLIVION_2026-07-02.md`.

**Delta since 2026-07-02**: Reviewed every commit landed since the prior sweep
(`ffe9a816`..`8498e559`, ~30 commits: ragdoll telemetry logging, pex decompiler
panic hardening, VWD record-header flag parsing, two-sided blend `z_write`
gate, Starfield `BSGeometry` sentinel-slot iteration, save/load referential
integrity, BLAS scratch-buffer lifecycle, and an audit-skill text refresh).
**None touch** `crates/nif/`, `crates/bsa/`, `crates/plugin/src/esm/`,
`byroredux/src/cell_loader/`, or `byroredux/src/material_translate.rs` in any
Oblivion-relevant way — the two closest (VWD flag parse, two-sided blend gate)
are cross-game and already tested. `git diff` over those paths for the window
is empty.

---

## Executive Summary

Oblivion compatibility remains in a **mature, regression-guarded state**, with
**zero code defects** found this sweep. This is the same conclusion as
2026-07-02, confirmed independently: no relevant code changed in the
intervening 30 commits, `nif_stats` reproduces byte-identical numbers, and
`cargo test --workspace` is fully green (0 failed across all crates).

One carry-over LOW documentation finding remains open in the audit skill
itself (not the codebase) — flagged yesterday as OBL-D1-NOTE-01 and **not
fixed** by the same-day skill-refresh commit (`8498e559`), which touched other
parts of Dimension 1 but left the erroneous sentence in place. Re-reported
below as it is still live in the checked-in skill file.

Current compatibility level (live numbers, this sweep):

| Layer | State (verified 2026-07-03) |
|-------|------------------------------|
| NIF parse (v20.0.0.5 + v10.x tail) | **99.93%** clean (8 026 / 8 032), recover 99.99%, **0 failures, 0 unknown block types** across 81 distinct types — `nif_stats` over `Oblivion - Meshes.bsa`, re-run live this sweep. Byte-identical to 2026-07-02 and to the ROADMAP compat-matrix row — zero drift. |
| Archive extract (BSA v103) | End-to-end; version gate (`BSA_V_OBLIVION = 103`, `open.rs:40`), 16-byte folder record (`open.rs:100`), `embed_file_names` guard (`open.rs:75`) all hold (#699 regression guard). |
| ESM parse (live path) | 16-byte ACBS Oblivion arm (#1650), 8-byte CLMT WLST (#540), pre-Skyrim XCLL, `is_oblivion` ATTR/DNAM/VNAM branches — all present + correct, unchanged since yesterday. |
| Render (legacy shaders) | NiTexturingProperty→MaterialInfo, raw monitor-space colors (no sRGB), NiWireframeProperty→LINE + flat_shading (#869), Disney BSDF gate stays 0 (no BGSM/BGEM ever authored by Oblivion). |
| NIFAL canonical translate | Metalness/roughness resolve-once via `Material::resolve_pbr`; no per-draw `classify_pbr`; `EmissiveSource::Material` tagged on the NiMaterialProperty arm. |
| Interior cell | Renders end-to-end (Anvil Heinrich Oaken Halls). |
| Exterior cell | TES4 worldspace + LAND wiring implemented and game-agnostic (parse + load ✓); Oblivion-aware (worldspace selector, NAM2 default water, climate). **Only an on-device exterior render bench remains.** |

**Top blocker (only one, unchanged)**: an on-device exterior render bench for a
TES4 worldspace. This is quality/verification, not missing code. The stale
"BSA v103 is broken" framing stays dead (#699) and was NOT regenerated.

---

## Dimension Findings

### Dimension 1 — NIF Version Handling (v20.0.0.5 + v10.x tail)

Re-verified all checklist items against `crates/nif/src/header.rs`,
`crates/nif/src/version.rs`, `crates/nif/src/blocks/controller/morph.rs`,
`crates/nif/src/blocks/properties.rs`, `crates/nif/src/import/collision.rs`.
`cargo test -p byroredux-nif` green (unchanged pass count from yesterday). No
new code defects.

- `user_version >= V10_0_1_8` gate at `header.rs:114` — holds.
- BSStreamHeader dual-band condition at `header.rs:137-143` matches nif.xml
  exactly, including the `#170` out-of-band non-Bethesda refusal.
- `#1509` gate confirmed **correct in code**: `morph.rs:89-92` —
  `version >= V10_2_0_0 && version <= V20_0_0_5 && bsver > 9`. `doghead.nif`
  (v10.2.0.0, bsver **9**) correctly evaluates `9 > 9 == false` and **skips**
  the trailing field (regression test `nigeommorpher_v10_2_bsver9_skips_trailing_unknown_ints`,
  `path_lookat_tests.rs:189`). Oblivion's bsver-11 morph rigs correctly **keep**
  the field.
- `NiTexturingProperty` still reads a raw `u32` count with no leading bool gate
  (`properties.rs`, regression guard for #149).
- `bhk motion_type` full-enum mapping (#1652) intact in `import/collision.rs`.
- `BhkMultiSphereShape` / `BhkConvexListShape` still translate via
  `resolve_shape_inner`, not dropped.

#### OBL-D1-NOTE-01 (carry-over): Audit-skill checklist item still contradicts correct code
- **Severity**: LOW
- **Dimension**: NIF Version Handling
- **Location**: `.claude/commands/audit-oblivion/SKILL.md` lines 110-113 (Dimension 1, `#1509` checklist bullet); code is correct at `crates/nif/src/blocks/controller/morph.rs:89-92`
- **Status**: Existing — first reported 2026-07-02 as OBL-D1-NOTE-01, **not fixed**. The same-day skill-refresh commit `8498e559` ("Refresh audit-* skill files to match current codebase state") edited other lines of this same Dimension-1 section (the parse-rate string, the `recovery_trace` truncation count) but left this specific sentence untouched — confirmed via `git show 8498e559 -- .claude/commands/audit-oblivion/SKILL.md`.
- **Description**: The skill checklist reads: *"`doghead.nif` is v10.2.0.0 **bsver 9** and must keep the field; an off-by-band gate restarts `NiMorphData` 24 B late and truncates the file."* This is backwards. The `#1509` fix gates the trailing `Num Unknown Ints` field on `bsver > 9`; doghead is bsver **9**, so `9 > 9` is false and the field is correctly **skipped**. The field is *kept* for Oblivion's bsver-**11** morph rigs (e.g. `obgatemini01.nif`), which is what the original `#687` fix relies on. The code, its inline comment (`morph.rs:75-84`), and the regression test name (`nigeommorpher_v10_2_bsver9_skips_trailing_unknown_ints`) all agree the gate is correct; only the audit skill's checklist prose is wrong.
- **Evidence**: `morph.rs:89-92` — `if version >= NifVersion::V10_2_0_0 && version <= NifVersion::V20_0_0_5 && bsver > 9`. `path_lookat_tests.rs:166-189` names and asserts the doghead-skips-the-field behavior directly.
- **Impact**: None on runtime — the code has been correct since `#1509` closed. Risk is purely to future audit hygiene: a future session (human or agent) reading only the skill checklist could "fix" the code to match the erroneous sentence and re-break doghead + its 15 trailing blocks (the original `#1509` regression shape).
- **Related**: #1509, #687, prior report `docs/audits/AUDIT_OBLIVION_2026-07-02.md` (OBL-D1-NOTE-01)
- **Suggested Fix**: Amend `.claude/commands/audit-oblivion/SKILL.md` lines 110-113 to: *"`doghead.nif` is v10.2.0.0 bsver 9 and must **skip** the trailing field; Oblivion's bsver-11 morph rigs must **keep** it — an off-by-band gate either direction truncates/misaligns `NiMorphData`."*

### Dimension 2 — BSA v103 Archive

Regression guard, re-confirmed. No findings. `BSA_V_OBLIVION = 103`
(`crates/bsa/src/archive/mod.rs:32`); version rejected outside {103,104,105}
(`open.rs:40`); folder-record size is `if version == BSA_V_SKYRIM_SE { 24 }
else { 16 }` (`open.rs:100` — v103 AND v104 both 16 B, correct);
`embed_file_names = version >= BSA_V_FO3_SKYRIM && flag` (`open.rs:75`, so
v103 never embeds names). `nif_stats` round-trips 8 032 NIFs through the v103
extract path with 0 extract failures, this sweep.

### Dimension 3 — ESM Record Coverage (live path)

No findings. Unchanged since yesterday — no commits touched
`crates/plugin/src/esm/` this window. Oblivion-specific decode branches
verified still present:
- 16-byte ACBS (#1650) — `actor.rs` `GameKind::Oblivion && len >= 16` arm gated
  before the FNV `len >= 24` arm.
- CLMT WLST (#540) — Oblivion 8-byte entries vs later 12-byte layout.
- Pre-Skyrim XCLL lighting layout in `cell/mod.rs`.
- `is_oblivion` ATTR/DNAM/VNAM/PNAM/UNAM/XNAM sub-branches in `actor.rs`.

### Dimension 4 — Rendering Path for Oblivion Shaders

No findings. Confirmed unchanged:
- No `srgb_to_linear` on legacy `NiMaterialProperty` colors.
- `NiWireframeProperty` → LINE polygon mode + `flat_shading` forwarding (#869)
  intact.
- Typed particle-emitter path (`apply_emitter_params`,
  `byroredux/src/systems/particle.rs`) still fed by the typed NIF emitter
  blocks. The `#1804` two-sided-blend `z_write` gate landed this window
  (`crates/renderer/src/vulkan/context/draw.rs`) — read the diff: it narrows
  `needs_two_sided_blend_split` to `is_blend && two_sided && z_write`, and
  particles (which are `z_write: false`) are the motivating case, not a
  regression risk for Oblivion's material universe (Oblivion has no
  z_write:true two-sided-blend content beyond what already worked, e.g.
  glass, which keeps its split).
- `is_pbr` remains hardcoded `false` on every NIF geometry import path
  (`mesh/ni_tri_shape.rs`, `mesh/bs_tri_shape.rs`, `mesh/bs_geometry.rs`) —
  Disney BSDF gate stays unreachable for Oblivion.

### Dimension 5 — NIFAL Canonical Material Translation

No findings. `byroredux/src/material_translate.rs` and
`crates/core/src/ecs/components/material.rs` unchanged this window.
Metalness/roughness resolve-once via `Material::resolve_pbr` confirmed;
`EmissiveSource::Material` tagging on the `NiMaterialProperty` arm confirmed
in `material/walker.rs`.

### Dimension 6 — Real-Data Validation

No findings — **zero drift**, re-run live this sweep:

```
total:       8032
clean:       8026  (99.93%)
truncated:      6  (38 blocks dropped)
failures:       0
recovered:      0
(81 distinct block types, 0 unknown)
```

Truncated set is byte-identical to the checked-in `oblivion_truncations.tsv`
baseline (#1611) and to yesterday's sweep: `marker_arrow.nif` (5 dropped),
`marker_map.nif` (8), `marker_radius.nif` (2), `marker_divine.nif` (9),
`marker_temple.nif` (9), `marker_travel.nif` (5) — all pre-Gamebryo NetImmerse
inline-type-name markers, the expected graceful-truncation case. No new block
types, no `parsed` shrinkage, no `unknown` growth.

### Dimension 7 — Exterior Blocker Chain & Quirks

No findings. `byroredux/src/cell_loader/` unchanged this window. Exterior
parse+load path remains wired and game-agnostic
(`cell_loader/exterior.rs`) with Oblivion-aware handling: worldspace selector
(#1655/#444), NAM2 default water, per-worldspace climate. The `#1731` VWD
record-header-flag parse/expose commit this window
(`crates/plugin/src/esm/reader.rs`) is additive (new named accessor,
`is_visible_when_distant()`) and explicitly scoped to *not* wire LOD-culling
behavior yet (that's a separate follow-up per its own commit message) — no
behavior change to the current Oblivion exterior/LOD path to audit here.
`_far.nif` distant-object LOD (#1726/#1745) code paths
(`cell_loader/object_lod.rs`, `cell_loader/placement_lod.rs`) also unchanged
this window.

---

## Blocker Chain (to "exterior cell renders")

Interiors already render end-to-end (Anvil Heinrich Oaken Halls). Unchanged
from yesterday:

1. TES4 worldspace + LAND wiring — **implemented, game-agnostic** (parse + load ✓).
2. CELL exterior REFR placement — **implemented** (shares the FNV-aligned path).
3. On-device exterior render bench for a TES4 worldspace (Tamriel) — **pending**.

Do NOT regenerate the stale BSA-v103 framing; v103 extraction has worked
end-to-end since 2026-04-17 (#699).

---

## Regression Guard List (verified still holding this sweep)

- **v10.x stride-drift family** — #1506 (NiInterpController/NiQuatTransform),
  #1507 (NiPSysData `!BS202` gate), #1508 (NiBlendInterpolator three-band
  split), #1509 (NiGeomMorpherController `bsver > 9`, doghead skips
  correctly): all land on the next block boundary; `nif_stats` truncation
  count steady at 6.
- **NiTexturingProperty raw u32 count** (#149) — no leading bool gate.
- **BSStreamHeader dual-band** (#170) — matches nif.xml; out-of-band refusal holds.
- **`user_version >= V10_0_1_8` threshold** — `header.rs:114`.
- **bhk motion_type full enum** (#1652) — `import/collision.rs`.
- **Collision import** — BhkMultiSphereShape / BhkConvexListShape translate, not dropped.
- **BSA v103 extraction** (#699) — 0 extract failures over `Oblivion - Meshes.bsa`.
- **16-byte ACBS** (#1650), **8-byte CLMT WLST** (#540).
- **Disney BSDF gate stays 0** — `is_pbr = false` on all NIF import paths.
- **Raw monitor-space legacy colors** (0e8efc6) — no sRGB on NiMaterialProperty.

---

## Verdict

Oblivion support remains healthy and unchanged in substance from the
2026-07-02 sweep. No new bugs. The single carry-over finding
(OBL-D1-NOTE-01) is a documentation-hygiene issue in the audit skill's own
checklist text, not a codebase defect — the code has been correct since
`#1509` closed. All regression guards independently re-verified against live
source and real vanilla data with zero drift; `nif_stats` numbers are
byte-identical to yesterday. The sole remaining substantive item is an
on-device exterior render bench — quality verification of already-wired code,
not a parser/archive/ESM gap.
