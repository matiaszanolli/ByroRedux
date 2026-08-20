# #3194 — SPT-D2-2026-08-20-03: SpeedTree is the one wind consumer with no non-finite guard

- **Filed**: 2026-08-20 (`/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3194
- **Labels**: `low,renderer,bug`
- **Source report**: `docs/audits/AUDIT_SPEEDTREE_2026-08-20.md`
- **HEAD at audit**: `bb0b92f2`

---

**Severity**: LOW
**Dimension**: Placeholder Fallback
**Source**: `docs/audits/AUDIT_SPEEDTREE_2026-08-20.md` (`SPT-D2-2026-08-20-03`) — HEAD `bb0b92f2`

## Location

- `byroredux/src/systems/billboard.rs` — `apply_speedtree_wind`'s gust / strength derivation
- `byroredux/src/render/water.rs` — the guarded gust
- `crates/physics/src/water.rs` — the guarded gust
- `crates/core/src/ecs/components/groundcover.rs` — `WindField::is_well_formed`, `WindField::CALM`

## Description

Both water consumers sanitise the instantaneous gust:

```rust
let gust = if gust.is_finite() { gust.max(0.0) } else { 0.0 };
```

and **both annotate that line as deference to SpeedTree**:

- `byroredux/src/render/water.rs`: *"SpeedTree treats a negative instantaneous gust as calm weather by
  clamping its bend strength to zero. Keep water's UV drift on that same one-sided magnitude contract"*
- `crates/physics/src/water.rs`: *"Match the renderer and SpeedTree wind contract."*

**Half of that is true.** `strength = (gust / MAX_WIND_SPEED).clamp(0.0, 1.0)` does floor negatives at
zero. **The other half is not**: Rust's `f32::clamp` returns `NaN` for a `NaN` input (it is
`if self < min … if self > max …`, both false for `NaN`), so a non-finite gust propagates straight
through `strength` → `bend` → `Quat::from_rotation_z` → `GlobalTransform.rotation`, poisoning the
entity's world transform and everything downstream that reads it (bounds propagation, instance upload,
BLAS/TLAS transforms).

Second-order symptom of the same input: `wind_state_changed = last_wind != Some(wind)` is derived
`PartialEq`, so a `NaN` anywhere in `WindField` makes it permanently `true` and the camera gate never
closes.

## Evidence

- The three sites above, confirmed at HEAD: two guarded, one not.
- `grep -rn "is_well_formed" crates byroredux --include='*.rs'` returns **only the definition and its own
  unit tests** — `WindField::is_well_formed` exists precisely to catch a malformed field at the
  translation boundary and has **zero production callers**.

**Not reachable in production today**: the only production producer is `WindField::from_weather_byte`
via `resolve_wind` (`groundcover_translate.rs`), which derives every field from a `u8` and a `cos`/`sin`
pair — all finite — and with `gust_amplitude = speed·(0.25 + 0.55·n) ≤ 0.8·speed` the gust cannot even go
negative. So this is a defense-in-depth gap **plus a documentation defect** (two comments assert a
contract the third site does not honour), not a live bug.

## Impact

A hand-authored, modded, or future procedurally-driven `WindField` with a non-finite field silently
produces `NaN` world rotations on **every tree**, where the same input produces calm water. The renderer
is documented as hard-failing non-finite environment values (EX-05, quoted in `groundcover.rs`), so this
would surface as a validation abort or garbage geometry rather than a graceful degrade.

## Suggested Fix

Apply the water sites' guard verbatim in `apply_speedtree_wind` — or, better, since three subsystems now
share the field, **hoist it**: call `WindField::is_well_formed` at the single install site
(`byroredux/src/scene/world_setup.rs`) and substitute `WindField::CALM` when it fails, so all three
consumers inherit one sanitised value and the two water-side comments become true. That also gives
`is_well_formed` its first production caller.

## Related

- **#3191** (`SPT-D2-2026-08-20-01`) — the same expression, different defect.
- **EX-05** — the renderer's non-finite environment-value contract.
- **#3132** (`SAFE-2026-08-20-01`, OPEN) — the same `f32::clamp`-is-NaN-transparent shape on the WATR
  decode path. Same root cause class, different subsystem.

## Completeness Checks

- [ ] **SIBLING**: if hoisted, all three consumers (billboard, render water, physics water) drop their
      local guards or keep them consistently — do not leave a fourth variant
- [ ] **TESTS**: a guard feeding a non-finite `WindField` and asserting `GlobalTransform.rotation` stays
      finite on every tree; and a guard that `is_well_formed` is actually called at the install site
