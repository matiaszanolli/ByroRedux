# Issue Batch: 3014, 3018, 3067, 3111

## #3014 — SCR-D8-2026-08-16-04 (byroredux-hkx)
`crates/hkx/src/animation.rs:887-899` — the crate's only integration test
(`skyrim_cart_player_idle_decodes_when_assets_are_available`) silently
`return`s (not `#[ignore]`) when `BYROREDUX_SKYRIM_DATA`/the BSA archive
isn't present, so it reports green with zero real coverage on any machine
without game data (including CI). No negative/malformed-input fixtures
exist at all. Suggested fix: mark the data-dependent test `#[ignore]`
(matching `crates/pex/tests/r5_fidelity.rs`'s pattern) and add checked-in
malformed-input fixtures covering bounds the crate doesn't enforce (ties to
#3011/#3013, already-fixed parser gaps this coverage gap allowed).

## #3018 — SCR-D8-2026-08-16-03 (byroredux-hkx)
`crates/hkx/src/animation.rs:331-333` vs `:341` — an out-of-range annotation
timestamp hard-fails the ENTIRE clip decode (`return Err(InvalidData(...))`),
but the very next lines show the codebase already knows how to tolerate the
same condition by clamping (`time: time.min(duration)`). Two policies
disagree. Suggested fix: pick one — since clamping already exists for
accepted values, skip/clamp the offending annotation and keep the clip
(log the anomaly) instead of discarding all transform tracks over one bad
metadata timestamp.

## #3067 — PHYS-D3-2026-08-16-04 (byroredux-physics)
`crates/physics/src/sync.rs:787-789` — `register_newcomers`'s
`parts.is_empty() { continue; }` skip is unreachable; every producer
(`collision_shape_to_parts`) pushes at least one part unconditionally.
Suggested fix: convert to `debug_assert!(!parts.is_empty())` to document the
producer contract (not silently delete) — relevant because #3066 touches the
same producer chain and could make this reachable later.

## #3111 — ECS-2026-08-20-01 (ecs + binary, HIGH)
`byroredux/src/boot.rs:691-724` (`player_controller_system`'s `Access`
chain) is missing a `WindField` read declaration. The read happens 3 call
frames down: `character.rs:911-916` → `byroredux_physics::water.rs:323-327`
`world.try_resource::<WindField>()`. `weather_system` (same `Stage::Early`
parallel batch, `boot.rs:726-740`) takes an unconditional
`try_resource_mut::<WindField>()` write every tick — an undeclared,
unsynchronized read/write race within one parallel batch. Not a deadlock
(no cycle), but breaks the M27 access-model invariant
(`known_conflict_count() == 0` ⇒ no undeclared same-stage read/write
overlap) silently, and is observably a controller-state strobe risk at the
swim threshold. Suggested fix (two options):
- (a) Move `weather_system` to `add_exclusive_with_access` — matches M27
  Phase 3 precedent (`audio_system`/`spin_system`), smaller change.
- (b) Hoist the WTHR wind update into an earlier stage.
Then add the honest `WindField` read declaration to
`player_controller_system` and confirm `known_conflict_count() == 0` still
holds. Also named as sibling declaration gaps (lower severity, no live
writer, filed separately — NOT in this batch): `physics_sync_system`,
`make_animation_system`, `make_billboard_system`.

## Domain classification
- #3014, #3018 → **byroredux-hkx**
- #3067 → **byroredux-physics**
- #3111 → **ecs** (byroredux-core scheduler/access model) + **binary**
  (byroredux boot.rs/systems/*.rs) — cross-domain, primary test target
  `byroredux` (binary) since the scheduling decision + declaration live
  there; `byroredux-core`'s access-conflict logic itself isn't being
  changed, just correctly fed.

## Plan
#3111 is HIGH severity and structurally significant — investigate first,
pick option (a) or (b) per its own suggested-fix guidance, and confirm scope
before implementing. #3014/#3018 are same-crate (hkx) siblings — natural to
implement together. #3067 is a single-site, low-risk assert-conversion.
