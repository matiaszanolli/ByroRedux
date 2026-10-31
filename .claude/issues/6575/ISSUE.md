# TOOL-D1-2026-10-31-01: `entity.move` and `place.at` parse `f32` offsets with no finite check, so `NaN`/`inf` is written into live transforms or a created placement

Labels: low,tech-debt,bug

Source: `docs/audits/AUDIT_TOOLING_2026-10-31.md` (TOOL-D1-2026-10-31-01). Severity LOW. Dimension: Debug Trust Boundary. Exposure: any local process on a debug build, or a release launched with `BYRO_DEBUG_SERVER=1` (the console is unauthenticated); developer-only in practice. Added by `93428cba1` / `7c9414845` (M43 live gates).

**Location**: `byroredux/src/commands/story_actions.rs:256-285` (`EntityMoveCommand::execute`), `:165-197` (`PlaceAtCommand::execute`, offset parse at `:171`)

## Description
`str::parse::<f32>` accepts `NaN`, `inf` and `-inf`. `entity.move <e> NaN 0 0` adds a NaN vector to `Transform.translation` **and** `GlobalTransform.translation` of any entity, and nothing downstream (transform propagation, physics sync, BLAS/TLAS instance upload) re-validates it. `place.at <base> NaN` builds `position = zup + offset` and hands it to `placement_at_position` and `CreatedReferences::place`, so the persistent created-reference registry then holds a non-finite position. `place.at` also uses `filter_map(|part| part.parse().ok())`, so `place.at 1F abc 5` silently reads `5` as `dx`: a typo shifts every later axis instead of being refused. The rest of the console follows the finite-check convention (`time.rs:38,195,275`, `terrain.rs:206`, `view.rs:570`, `hud.rs:95,143`, `physics.rs:68`, `anim_event.rs:161-216`, `scene.rs:962`; `studio_host::valid_transform` for the SDK path).

## Evidence
`story_actions.rs:259-262` and `:171`; no `is_finite` anywhere in `story_actions.rs`.

## Impact
One console line from a local process, or a typo in a live gate script, can leave a NaN transform on an entity (renderer instance data, Rapier body sync) or a non-finite created reference that a save then persists. Recoverable only by reloading the cell. Developer tooling, hence LOW.

## Suggested Fix
Reject non-finite deltas in `entity.move` and non-finite or unparsable offsets in `place.at` with the usage error, as `terrain.rs:206` does. One test per command (`entity.move 1 NaN 0 0` leaves the transform unchanged and returns an error).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
