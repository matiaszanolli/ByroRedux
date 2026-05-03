**Severity**: HIGH (likely root cause for distance-based bright/chromy artifact reported on FNV interior cells, 2026-05-01 user session)
**Dimension**: Sky/Weather/Exterior Lighting × Composite
**Source**: AUDIT_RENDERER_2026-05-01_FOCUS.md

## Locations
- [byroredux/src/systems.rs:1493-1501](../../tree/main/byroredux/src/systems.rs#L1493-L1501) — `weather_system` writes `fog_color`/`fog_near`/`fog_far` into `CellLightingRes` unconditionally
- [byroredux/src/render.rs:1056-1069](../../tree/main/byroredux/src/render.rs#L1056-L1069) — `build_render_data` reads those values for composite UBO upload
- [crates/renderer/shaders/composite.frag:293-308](../../tree/main/crates/renderer/shaders/composite.frag#L293-L308) — composite blends `fog_color` at up to 70% opacity into distant pixels in HDR linear space, pre-ACES
- [byroredux/src/scene.rs:384](../../tree/main/byroredux/src/scene.rs#L384) — exterior fallback `FOG_COLOR = [0.65, 0.7, 0.8]` (sky-blue)
- [byroredux/src/components.rs:131,157](../../tree/main/byroredux/src/components.rs#L131) — `is_interior: bool` / `is_exterior: bool` already tracked elsewhere

## Description

`weather_system` runs every frame, derives a current `(fog_color, fog_near, fog_far)` triple from `WeatherDataRes` (populated when an exterior cell loads), and writes them into `CellLightingRes`:

```rust
// systems.rs:1494-1501 — current state, NO interior gate
if let Some(mut cell_lit) = world.try_resource_mut::<CellLightingRes>() {
    cell_lit.ambient = ambient;
    cell_lit.directional_color = sunlight;
    cell_lit.directional_dir = sun_dir;
    cell_lit.fog_color = fog_col;       // unconditional
    cell_lit.fog_near = fog_near;       // unconditional
    cell_lit.fog_far = fog_far;         // unconditional
}
```

Verified via `grep -n "is_interior\|cell_lit.*interior" byroredux/src/systems.rs` returning **zero hits** — there is no interior/exterior guard on these writes.

## Symptom chain

1. Player visits (or pre-loads) any exterior worldspace → `WeatherDataRes` populated with sky-blue WTHR fog
2. `weather_system` runs every frame and writes that fog into `CellLightingRes`
3. Player enters an interior FNV cell → cell_loader sets the cell's XCLL fog into `CellLightingRes` initially…
4. …but next frame's `weather_system` clobbers it with the lingering exterior fog
5. `composite.frag:307` blends `fog_color` at up to 70% opacity into distant pixels in HDR linear space, pre-ACES → distant interior surfaces wash toward sky-blue/chromy
6. ACES tone mapping squashes the bright fog-mixed values, producing the posterized look

## Visual signature (matches user's 2026-05-01 screenshots)

- Foreground correct, distant surfaces chromy/over-bright
- Sharp transitions along depth contours
- Posterization / stair-stepping at the brightness boundary
- Slight blue-grey tint to the over-bright regions (matches `[0.65, 0.7, 0.8]` fog default)
- Pattern is consistent across multiple interior FNV cells

## Diagnostic test (no code change required)

Launch the engine *directly* into the affected interior cell as the **first cell loaded this process** — do not visit any exterior worldspace beforehand. If the chromy distance look disappears, this finding is the confirmed root cause. If the look persists on a fresh-launch interior, secondary causes (LIGHT-N2 = HDR-pre-tonemap fog math, CSTC-N1 = caustic distance bias) need investigation.

## Suggested Fix

Add an `is_exterior: bool` field to `CellLightingRes` (the engine already tracks both flags in `components.rs`), and gate the fog writes on it:

```rust
if let Some(mut cell_lit) = world.try_resource_mut::<CellLightingRes>() {
    cell_lit.ambient = ambient;
    cell_lit.directional_color = sunlight;
    cell_lit.directional_dir = sun_dir;
    // Only overwrite fog when the active cell is exterior — interior
    // cells preserve their XCLL/LGTM-authored fog from cell_loader.
    if cell_lit.is_exterior {
        cell_lit.fog_color = fog_col;
        cell_lit.fog_near = fog_near;
        cell_lit.fog_far = fog_far;
    }
}
```

Plus the field plumbing: `cell_loader.rs` should set `is_exterior = false` for interior cells when populating `CellLightingRes`. Both `interior` and `exterior` flags exist already on `cell` types — propagation surface is small.

## Impact

Every interior FNV cell loaded after any exterior cell session shows wrong fog. Highly visible visual regression. Likely root cause of the user's 2026-05-01 reported "distance lighting goes from 0 to 100" symptom that persisted across the full revert chain of the #779 attempts.

Ambient and directional values written to `CellLightingRes` (lines 1495-1497) may have similar interior-leak issues but are out of scope for this finding — flag separately if observed.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Verify `cell_lit.ambient`, `cell_lit.directional_color`, `cell_lit.directional_dir` (also unconditionally overwritten in the same block) — these may need the same gate, or may be intentionally global-time-of-day-driven. Investigate before deciding.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add a unit test that verifies `weather_system` does NOT mutate `cell_lit.fog_color` when `cell_lit.is_exterior == false`. Either via a focused ECS test in `systems.rs` test module, or as a regression integration test that mirrors the diagnostic-test scenario (load interior cell → run weather_system → assert fog_color unchanged).
