# Investigation — #1340 (D3-04): runtime interior loads discard CellLoadResult.lighting

## Root cause (confirmed)
`load_cell_with_masters` resolves the cell's XCLL/LGTM lighting and returns it as
`CellLoadResult.lighting: Option<CellLighting>` (`load.rs:30`) but does NOT install the
`CellLightingRes` resource — that is the caller's job. Of the three interior-load entry points,
only the startup `--cell` path did it:

- `scene.rs` (startup) — read `result.lighting`, computed the directional dir, inserted
  `CellLightingRes::from_cell_lighting(lit, dir, is_interior=true)`. ✓
- `transition.rs::load_interior_cell` (M40 door-walk) — `…load_cell_with_masters(…).map_err(…)?;`
  discarded the `CellLoadResult` entirely. ✗
- `debug_load.rs::exec_load_interior` (`cell.load` console cmd) — captured `result` but read only
  `entity_count`/`center` for logging. ✗

So any interior reached at runtime kept the *previous* cell's `CellLightingRes`: wrong
ambient/fog, exterior clear color, and the directional sun leaking into a sealed interior — the
exact failure #1282 gated on `is_interior` (which stays `false` on a stale exterior resource).

## Fix: one shared helper, three callers (no duplication)
Extracted the lighting-apply logic from `scene.rs` into
`cell_loader::load::apply_interior_cell_lighting(world, &CellLighting)` (re-exported at the
`cell_loader` level), and routed all three call sites through it. The helper hardcodes
`is_interior = true` (`load_cell_with_masters` is interior-only) so the directional sun is
always gated out of a sealed cell. This satisfies the global "improve, don't duplicate"
guidance and makes the three paths impossible to drift again.

## Sibling completeness
Grepped every real `load_cell_with_masters` call site: exactly three (scene.rs:179,
transition.rs:222, debug_load.rs:198) — all now call `apply_interior_cell_lighting`. No missed
interior entry point. Exterior cells use a different worldspace/weather lighting path
(`apply_worldspace_weather`), out of scope here.

## Test
`apply_interior_cell_lighting` is GPU-free (inserts an ECS resource, no `VulkanContext`), so it
is unit-tested directly in `load.rs` (`apply_interior_cell_lighting_inserts_interior_resource`):
a fresh world (no prior `CellLightingRes`) → after the call the resource exists with
`is_interior == true` and the fog/ambient propagated. The transition/debug call-site wiring
needs a `VulkanContext` and isn't unit-testable; the shared-helper consolidation + the sibling
grep are the structural guard there.

## Files (5)
- `cell_loader/load.rs` — `apply_interior_cell_lighting` helper + import + test.
- `cell_loader.rs` — `pub(crate) use load::apply_interior_cell_lighting`.
- `scene.rs` — replace the inline block with the helper call; `CellLightingRes` import now
  `#[cfg(test)]` (only the `scene::*` test submodules still name it via `use super::*`).
- `cell_loader/transition.rs` — capture the result, apply lighting.
- `debug_load.rs` — apply lighting in the `Ok(result)` arm.
