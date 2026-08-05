# Batch: #2292, #2293, #2294, #2295

## #2292 — SAVE-D1-09: PlayerControlState/ActorControlState absent from build_save_registry

**Fix**: Added `#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]`
to both types, registered `PlayerControlState` as a resource and `ActorControlState` as a
component (also added to `MUTABLE_DELTA_COLUMNS` — single bool, delta-safe). Round-trip
regression test added.

## #2293 — SAVE-D1-10: Dead actor-lifecycle marker unregistered (forward-latent)

**Fix**: No registration — the issue itself states none is needed today (zero live
inserters outside tests). Added a doc comment on `Dead` documenting this as a deliberate,
tracked decision with an explicit "register the moment a real system inserts it" tripwire,
so the omission reads as documented, not merely missed.

## #2294 — SAVE-D1-11: Scene/Dialogue/Package mid-playback progress undocumented omission

**Fix**: Added `#1696`-style doc comments to `ScenePlayer`, `DialoguePlayback`, and
`ScenePackagePlayback` stating the believed rationale (re-derived via `QuestStageState` +
`SceneStartRequest` on reload) — chose documentation over registration per the issue's own
"either/or" framing, matching the low-risk path. Also documented the SIBLING
MQ101-demo-scoped `ActorCinematicState`/`HorseTetherState`/`CinematicPresentationState`
in `cinematic.rs`. **Correction while building #2295**: the "cinematic" trio's rationale
did NOT hold under investigation — filed as new gaps (#2380). Added a caveat to
`ScenePlayer`'s doc comment cross-referencing this.

## #2295 — SAVE-D1-12: Registry-completeness guard covers only NPC-spawn-stamped components

**Fix**: Built `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted`, a
source-scan guard (no reflection) that walks every `.rs` file under
`crates/core/src/ecs/components/`, `crates/scripting/src/`, and `crates/physics/src/`,
extracts every `impl Component for X` / `impl Resource for X`, and requires each `X` to be
registered in `build_save_registry` XOR listed in a new `NOT_SAVED_BY_DESIGN` allowlist
with a one-line reason — mirroring the existing `REDERIVED_NOT_SAVED`/`AUDITED` tripwire
philosophy, generalized past the NPC-spawn-stamped surface.

Classified all 132 Component/Resource impls across the three directories (verified via
parallel research passes, cross-checked for complete coverage — zero types unaccounted
for). Sanity-checked the guard actually fails on a broken/missing allowlist entry (not
vacuously passing).

**7 previously-untracked, genuine save gaps surfaced by building this guard** — filed as
new issues rather than fixed inline, since each needs its own per-field delta-safety
review before registration (matching the care #1834/#2291/#2292 each took):
- #2378 (SAVE-D1-13) — `Material` live-mutated via the `mat.set` debug console command.
- #2379 (SAVE-D1-14) — `RigidBodyData.motion_type` mutated by scripted `SetMotionType`.
- #2380 (SAVE-D1-15) — `ActorCinematicState`/`CinematicPresentationState`/`HorseTetherState`,
  the MQ101 cinematic trio, live-mutated by Papyrus fragment effects with no reload
  re-derivation (the #2294 "believed self-correcting" assumption does NOT hold for these).
- #2381 (SAVE-D1-16) — `FragmentExecutionQueue`, suspended `Utility.Wait`/
  `WaitForActors3DLoaded` continuations.
- #2382 (SAVE-D1-17) — `RumbleOnActivate`, a live Active/Busy/Inactive gameplay state
  machine.

Also added `SaveRegistry::resource_names()` (sibling of the existing `component_names()`)
to `crates/save/src/registry.rs`, needed by the new guard.
