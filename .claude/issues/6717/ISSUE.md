# RF7-2026-11-05-07: `cell_loader` depends on `systems` for light translation and runtime helpers

Labels: low,tech-debt,bug,nifal

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 7
**Status**: NEW. Part of #6680

## Location
- `byroredux/src/systems/light_anim.rs:80` `canonical_light_animation_flags`, `:108` `canonical_light_shadow_flags`, `:152` `translate_light`, `:228` `canonical_light_falloff_exponent`
- Also used: `stop_script_music`, `raise_actor_attach_story_event`, `seat_world_transform`, `apply_seat_assignments`, `ground_character_body_at`, `apply_emitter_overlays`

## Why
This is the other half of the #6715 cycle. The light functions are translation code that lives under `systems`, so both the loader and the renderer reach up for them. The remaining helpers are runtime behaviour that the loader calls directly.

## Change
Move the light translation functions to the translation tier. For each remaining helper, either move it down or record why the call stays. Do this after #6714 and #6715, which remove the other references.

## Constraints
Move items verbatim. Run `grep -rn include_str` for each source file first. This does not propose splitting the binary crate. `every_ligh_spawn_site_consumes_the_canonical_falloff_lane` scans for these names.

## Acceptance Criteria
- The light translation functions no longer live under `systems`
- Each remaining `cell_loader` to `systems` reference has a stated reason
- Non-timing tokens of the `bench:` line match a baseline build of the parent commit

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser-to-`Material` boundary, never in shaders or at render time
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6717 --json state`)._
