# Issue #2774
title:	REN-D1-05: shrink_tlas_scratch_to_fit case-2 live-slot realloc arm appears unreachable
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, low, renderer, tech-debt
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	2774
--
## Description
The live-slot realloc arm appears unreachable — `current` and `peak` are written together in `ensure_tlas_state` and differ by ≤ `scratch_align − 1`, so `current > 2 × peak` cannot hold. All reclamation flows through case 1. Unit tests on the predicate give false confidence #1226 revived it. Confirm with a one-shot `log::debug!` before touching; **do not** change the shrink/destroy ordering (that is the #1782-class safety property).

## Location
`crates/renderer/src/vulkan/acceleration/memory.rs` (`shrink_tlas_scratch_to_fit`, case 2)

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D1-05).


---

# Issue #2795
title:	REN-D7-2026-08-12-01: main.rs debug_assert_eq panics on MAX_MATERIALS over-cap where a degrade path is already implemented
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, low, renderer
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	2795
--
## Description
Three authorities describe the `MAX_MATERIALS` over-cap path as a supported degrade (id 0 + warn-once); `main.rs` then `debug_assert_eq!`s the overflow count is zero, so a plain `cargo run` on a large/modded exterior **panics** where the degrade is already implemented, tested and documented. The same doc records the opposite call for `MAX_INSTANCES` (#956/#992). Reachable per the code's own recorded Skyrim radius-3 measurement (4000+ unique materials). Debug builds only.

## Location
`byroredux/src/main.rs` vs. `crates/renderer/src/vulkan/material.rs` + `docs/engine/memory-budget.md`

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D7-2026-08-12-01).


---

# Issue #3073
title:	NIFAL-D1-2026-08-16-01: parallax_height_scale / parallax_max_passes bypass the canonical Material, with the same magic defaults duplicated at six sites plus a render-time fallback
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, esm-plugin, medium, nif-parser, nifal, renderer
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	3073
--
Filed from `docs/audits/AUDIT_NIFAL_2026-08-16.md` (Dimension 1 — canonical boundary).

**Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs`:613-614 and five sibling sites · render-time fallback in the renderer

## Description

`parallax_height_scale` / `parallax_max_passes` **bypass the canonical `Material`**, with the same magic defaults duplicated at six sites plus a render-time fallback.

This is a NIFAL boundary violation in the precise sense the spec names: a material property that should be resolved once at the parser→`Material` boundary is instead re-derived at multiple downstream sites, including at render time.

## Evidence

The canonical home exists and is typed for it:
```rust
// crates/core/src/ecs/components/material.rs:402-403
pub parallax_max_passes: Option<f32>,
pub parallax_height_scale: Option<f32>,
```

But the values are also carried as plain `f32` on the renderer side and defaulted independently:
```rust
// crates/renderer/src/vulkan/context/mod.rs:147,150
pub parallax_height_scale: f32,
pub parallax_max_passes: f32,
// :421-422 — copied forward again
// crates/renderer/src/vulkan/water.rs:805-806 — a third default pair
```

Re-verified 2026-08-17.

## Impact

Six duplicated defaults mean six places to change and six chances to diverge — and the render-time fallback means a material that resolved one value at import can render with another.

Concretely relevant to #2997: FO4 slot-3 palette gradients currently reach `parallaxMapIndex`, and `GpuMaterial::default()`'s `parallax_height_scale = 0.04` is what makes the POM branch unconditionally live. Consolidating the value is part of making that fix legible.

## Suggested Fix

Resolve both values once in `Material::resolve_pbr` (or `translate_material`), store them on the canonical `Material`, and have every consumer read them from there. Delete the render-time fallback — per `docs/engine/nifal.md`, no per-game or per-material logic may be re-derived at render time.

## Related

- **#2997 (FO4-D5-06 — the POM branch this feeds)**
- `docs/engine/nifal.md` (the spec this violates)

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Values resolved once at the parser→`Material` boundary, never re-derived at render time
- [ ] **NO-DUPLICATION**: One default, not six
- [ ] **SIBLING**: `water.rs`'s pair included in the consolidation
- [ ] **SHADER-SYNC**: If `GpuMaterial` changes, the GLSL mirror in `bindings.glsl` moves in lockstep
- [ ] **TESTS**: A regression test asserts one authored value survives to the GPU struct


---

# Issue #3187
title:	NIFAL-D8-2026-08-20-02: RefrTextureOverlay::apply_slot_swap is a third slot table, game-agnostic, and its FO4 slot-5 arm reads a lane the FO4 TXST parser never populates
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, esm-plugin, game:fo4, low, nif-parser, nifal, renderer
comments:	1
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	3187
--
Filed from `docs/audits/AUDIT_NIFAL_2026-08-20.md` (Dimension 8 — shader-flags / texture-role vocabulary). NIFAL canonical-translation finding — see `/audit-nifal`.

**Severity**: LOW
**Tier violated**: `single-boundary` (a per-game slot vocabulary re-implemented outside `slot_to_role`)
**Game Affected**: FO4, FO76, Starfield

**Location**: `byroredux/src/cell_loader/refr.rs:158-183`

## Description

`RefrTextureOverlay::apply_slot_swap` maps a raw `XTXR` NIF-slot index onto a named `esm::cell::TextureSet` field with a flat, shader-type- and **game-agnostic** match. Its doc justifies the flatness with:

> *"The source TXST has already been translated from its different TXnn ordering into named roles, so this match is intentionally NIF-role order rather than raw ESM index order."*

That premise is only **half** true: the TXST -> named-role translation is itself game-dependent. `crates/plugin/src/esm/cell/support.rs:462-471` routes `TX02` to `set.wrinkle` for `Fallout4 | Fallout76 | Starfield` and to `set.env_mask` otherwise — so on those three games `set.env_mask` is **never populated**, while `apply_slot_swap(slot_index = 5)` reads exactly `ts.env_mask`.

Meanwhile `slot_to_role((Fallout4, 5))` on the tint family resolves to `TextureRole::Wrinkle` (`crates/nif/src/import/material/slot_role.rs:301-308`) — the role that lane should have reached.

## Evidence

```rust
// crates/plugin/src/esm/cell/support.rs:462-471
b"TX02" => { if matches!(game, Fallout4 | Fallout76 | Starfield) { set.wrinkle = path; }
             else { set.env_mask = path; } }

// byroredux/src/cell_loader/refr.rs:164 + :179   (no `game` in scope at all)
5 => ts.env_mask.as_deref(),        // <- always None on FO4/FO76/Starfield
5 => &mut self.env_mask,
```

The **non-`XTXR`** path is unaffected: `merge_from_texture_set` (`byroredux/src/cell_loader/refr.rs:130`) fills `self.wrinkle` from `ts.wrinkle` directly, and `byroredux/src/cell_loader/spawn/mesh_instance.rs:172` forwards it unconditionally. Only the explicit slot-index swap form loses the binding.

## Impact

An FO4/FO76/Starfield REFR that overrides NIF slot 5 via `XTXR` is a **silent no-op** instead of a wrinkle-map swap. Narrow — `XTXR` slot-5 swaps on head meshes are the only population — and it **fails closed** (nothing wrong is bound), which is why this is LOW rather than MEDIUM.

The maintenance cost is the real one: this is a **fourth** place the slot vocabulary is written down, after `slot_to_role`, the FO4 `TX02` branch, and the `mesh_instance.rs` `pick(...)` list.

## Suggested Fix

Give `apply_slot_swap` the game/layout it is missing and route slot 5 to `self.wrinkle` when the layout is FO4-family. Better: add `pick(5, o.wrinkle, TextureRole::Wrinkle)` alongside the existing `EnvironmentMask` pick in `mesh_instance.rs` and have `apply_slot_swap` write slot 5 into both lanes, letting `slot_to_role` remain the **sole arbiter** of the slot vocabulary.

## Related

- #2695 — the two-table defect.
- The `texture_slot_layout` discriminator finding from this same sweep — same "per-game routing decided outside the shared table" root cause.
- #2999 — introduced the FO4 slot-5 -> `Wrinkle` arm without a matching overlay-side path.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: the fix reduces the number of places the slot vocabulary is written down; `slot_to_role` stays the sole arbiter rather than gaining a fifth parallel copy. See `/audit-nifal`.
- [ ] **SIBLING**: the other seven slot indices audited for the same game-dependent TXST-lane mismatch, not only slot 5
- [ ] **TESTS**: an FO4 REFR with an `XTXR` slot-5 swap asserts the wrinkle lane is bound


---

