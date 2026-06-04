# Investigation — #1380 PERF-D4-NEW-04 animate_lights_system alloc + lock cycling

**Domain:** ecs / animation (CPU per-frame system)

## Finding
`animate_lights_system` collected a fresh `Vec<LightUpdate>` each frame
(per-frame-alloc), then re-acquired LightSource-write and Transform-write in two
more passes. As an exclusive Stage::Update system there's no concurrent-writer
reason to split read-then-write.

## Fix
- Extracted the intensity-modulation math into a pure
  `flicker_intensity(entity, flags, &flicker, total_time) -> f32` helper
  (unit-testable without a World; identical math).
- Collapsed passes 1+2 into a single `query_2_mut::<LightFlicker, LightSource>`
  (read flicker + write light together; distinct storages, TypeId-sorted
  internally → LOCK_ORDER-safe). Intensity is written in place — no
  `Vec<LightUpdate>` allocation, no read-then-write lock cycling.
- Pass 3 (Transform jitter) was provably dead: `translation` was hardcoded
  `None` (jitter disabled Phase 19.5). Removed it and the `LightUpdate` struct;
  `Transform` import dropped. A comment documents that re-enabling jitter means
  adding back a separate Transform write pass (kept separate so the live
  intensity path stays a two-storage query). `movement_amplitude` /
  `base_translation` remain parsed on `LightFlicker`.

Output is byte-identical: same entities (query_2_mut intersects the same set),
same per-entity intensity (same math).

## Completeness
- [x] UNSAFE: none
- [x] LOCK_ORDER: query_2_mut handles TypeId-sorted acquisition; system is
  exclusive in Stage::Update so no cross-system ordering hazard
- [x] SIBLING: no other system used the collect-then-write-twice pattern on these
  components
- [x] TESTS: 4 unit tests on `flicker_intensity` (no-flag unit, pulse sine, slow
  half-rate, flicker determinism + bounds)

## Verification
cargo test 2798 passed; no warnings in touched file.
