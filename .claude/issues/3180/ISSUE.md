# #3180 — AUD-2026-08-20-D6-01: submersion_system reads a camera pose camera_follow_system writes later the same frame

- **Filed**: 2026-08-20 (`/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3180
- **Labels**: `low,legacy-compat,bug`
- **Source report**: `docs/audits/AUDIT_AUDIO_2026-08-20.md`
- **HEAD at audit**: `bb0b92f2`

---

**Severity**: LOW
**Dimension**: Manager Lifecycle, ECS Lifecycle & Cell Streaming
**Source**: `docs/audits/AUDIT_AUDIO_2026-08-20.md` (`AUD-2026-08-20-D6-01`) — HEAD `bb0b92f2`

## Location

- `byroredux/src/boot.rs` — `submersion_system` registration (`Stage::PostUpdate`)
- `byroredux/src/boot.rs` — `camera_follow_system` registration (`Stage::Late`) and the comment above it
- `byroredux/src/systems/water.rs` — `submersion_system`'s own placement comment

## Status

NEW. The registration predates this cycle (`8a404914`), but it only became an *audio* dependency with
`75ad0653`, and no prior audio report covered `submersion_system`.

## Description

The comment above `camera_follow_system` states:

> *M28.5 — camera follow runs in Stage::Late, AFTER `physics_sync_system` has settled the kinematic
> body's post-step pose. **Must run BEFORE `audio_system` / `submersion_system`** (both read camera
> GlobalTransform).*

It runs before `audio_system` — both are `Stage::Late`, and the parallel batch completes before the
exclusives, so the listener pose is correct. It does **not** run before `submersion_system`, which is
registered in `Stage::PostUpdate` — an earlier stage entirely.

`submersion_system`'s own comment compounds the error: *"runs in PostUpdate after bound propagation so
the camera's GlobalTransform is already current for the frame"*. That is true only in **fly-cam** mode,
where the fly camera writes `Transform` in `Stage::Update` and PostUpdate propagation resolves it.

In **player / third-person** mode `camera_follow_system` is the pose author and writes both `Transform`
and `GlobalTransform` directly ("to bypass the missing late-stage propagation pass", per its own
comment). The value `submersion_system` reads at PostUpdate is therefore the **previous frame's** camera
pose — it predates both this frame's `Stage::Physics` step and this frame's camera follow.

## Evidence

Stage order is `Early → Update → PostUpdate → Physics → Late`.
`grep -n "Stage::" byroredux/src/boot.rs` puts `submersion_system` in `Stage::PostUpdate` and
`camera_follow_system` in `Stage::Late`.

Consumer chain: `submersion_system` → `SubmersionState.head_submerged` → `water_audio_system`
(`byroredux/src/systems/audio.rs`) → `AudioWorld::set_underwater` → `update_underwater_filters`.

## Impact

Exactly one frame (~16 ms) of latency on the underwater low-pass transition, and on the underwater
composite tint that reads the same state — **player mode only**. Below audibility on a normal wade-in.

The real cost is the **comment**: it asserts an ordering guarantee that does not hold, and the next
person hardening this chain (per-cell acoustics, occlusion, a submerged-listener reverb send) will
reason from it and be wrong. Both comments should be corrected even if the stage placement is left
where it is.

## Suggested Fix

Preferred: move `submersion_system` to `Stage::Late` as an exclusive registered immediately after
`camera_follow_system` and before `water_audio_system`. It already writes only `SubmersionState` plus
transient markers, so the move costs nothing and makes the `boot.rs` claim true instead of aspirational.

Otherwise: leave the stage and correct **both** comments to state that the camera pose is one frame
stale in player mode.

## Related

- **#3087** (OPEN) — stale audio scheduler-wiring comments, in the adjacent block of the same file.
  Worth fixing in the same pass.
- **#3086** (OPEN), **#3088** (OPEN) — the other two carried audio findings.
- The fly-cam-only correctness of the PostUpdate placement is why this has never produced a visible
  symptom.

## Completeness Checks

- [ ] **SIBLING**: if the stage moves, check every other `SubmersionState` reader (composite tint, water
      damage) for an assumed PostUpdate availability
- [ ] **LOCK_ORDER**: a `Stage::Late` exclusive registration keeps the documented Late exclusive order
      (ragdoll → water_damage → water_interaction → water_audio → audio_system → event_cleanup)
- [ ] **TESTS**: a guard pins the ordering claim the comment makes (a `scheduler_access_tests.rs`-style
      registration-order assertion), so the comment cannot re-rot
