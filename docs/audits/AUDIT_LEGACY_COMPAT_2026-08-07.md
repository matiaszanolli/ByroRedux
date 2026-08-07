# Legacy Compatibility Audit — Physics Closeout — 2026-08-07

**Base:** `7a851ab9` · **Type:** focused legacy-compat / PHYSAL boundary pass

**Scope:** The NIF collision-object translation boundary, cell-loader
fallback policy for opaque FO4+/FO76/Starfield packed Havok, and classic-chain
ragdoll extraction diagnostics. This is a focused implementation closeout, not
a comprehensive re-audit of every NIFAL/EXAL/PHYSAL dimension.

**Method:** Re-checked the canonical collision flow from block dispatch through
`CollisionAuthoring`, `CachedNifImport`, placement spawn, and Rapier sync.
Compared the shipped behavior against the current open physics/collision issue
inventory and the documented PHYSAL contract. The codebase knowledge-graph
service was unavailable (`Transport closed`), so discovery fell back to local
source inspection as permitted by the repository instructions. No GitHub issue
state was mutated by this audit.

## Remediated findings

| ID | Severity | Previous failure | Closeout |
|---|---:|---|---|
| #2355 | MEDIUM | `BhkNPCollisionObject` fallback covered only `RenderLayer::Architecture`; packed-Havok clutter/actors could remain non-colliding. | `CollisionAuthoringSummary` now survives the parse/import cache boundary and drives a layer-aware spawn policy. Architecture retains precise static trimeshes. Confirmed packed Clutter/Actor content receives one conservative keyframed AABB proxy, parented to the visual placement and excluded from rendering. Per-cell approximated/unresolved counts make residual coverage visible. |
| #2332 | LOW | FO3 DLC `bhkSPCollisionObject` dispatched as `BhkCollisionObject`, erasing phantom semantics. | Dispatch now uses the byte-identical `BhkPCollisionObject` wrapper. A byte-exact regression test pins the 10-byte layout and phantom downcast. |
| #2333 | LOW | `CollisionAuthoring` was diagnostic-only; fallback behavior did not consume the classification. | `summarize_collision_authoring` reuses the classifier, cache entries retain the summary on synchronous and streaming imports, and the runtime fallback policy consumes it. Policy tests prove packed authoring changes Clutter/Actor behavior while no-authoring Clutter remains non-colliding. |
| #2339 | LOW | Four ragdoll rejection paths were silent: unhosted bodies, unresolved shapes, non-finite body data, and unresolved constraint endpoints. | Each path now emits an actionable warning with block/bone/ref context. Non-finite constraints and self-links are also explicit; the diagnostic walk is gated on actual constraint authoring so ordinary rigid-body NIFs stay quiet. Existing finite/non-finite graph tests continue to pin the functional rejection behavior. |

## Boundary assessment

- **Single producer preserved.** Concrete NIF collision-object types are
  classified once by `classify_collision_block`; per-reference inspection and
  scene summarization share it.
- **Canonical sink preserved.** Decoded shapes still become only
  `CollisionShape` + `RigidBodyData`; the fallback creates those same canonical
  components and does not leak Havok/Rapier types into the cell loader.
- **Approximation is explicit.** The packed blob is not reinterpreted. Its
  authoring signal selects renderer geometry as a temporary proxy, and runtime
  telemetry distinguishes approximated from unresolved placements.
- **Transform ownership is explicit.** Packed proxies are keyframed children of
  the placement root, so a moving visual placement and its collider cannot
  drift apart. Unknown mass/motion data is not fabricated.
- **Decoded data wins.** Any imported collision suppresses compatibility
  synthesis. Decals, alpha-tested cards, fire refraction, empty geometry, and
  non-finite geometry cannot seed a packed proxy.

## Existing limitations re-confirmed

- **FO4+/FO76/Starfield packed-body fidelity remains blocked.** Decoding
  `BhkSystemBinary` is still required for authored mass, motion type, filters,
  constraints, destructibles, and ragdolls. The new AABB/triangle proxies close
  collision-presence gaps; they are not a binary decoder.
- **NIF-authored phantoms remain parse/classification only.** The specialised
  FO3 wrapper is now classified correctly, but `BhkPCollisionObject` still needs
  a Rapier-sensor/`TriggerVolume` consumer. This is separate from the existing
  ESM XPRM scripting-trigger path.
- **FO3 DLC archive baselines (#2334) remain open.** The parser regression is
  now pinned synthetically, but the optional real-data archive sweep still
  samples only the main mesh archive and does not enforce the DLC occurrence
  counts.
- **Exterior readiness/terrain ownership (#2375/#2377 family) remains a
  separate streaming milestone.** This closeout does not broaden the physics
  task into exterior cell orchestration.

## Verification

- `cargo test -p byroredux-nif collision --lib` — 100 passed.
- `cargo test -p byroredux synthesize_trimesh_tests` — targeted fallback
  policy/geometry/ECS ownership suite passed.
- `cargo check -p byroredux` — passed after cache and telemetry integration.
- `cargo test --workspace` — passed (unit, integration, and doctest suites;
  game-data-only tests remain ignored by design).
- Strict clippy passed for `byroredux-nif` + `byroredux` after allowing four
  pre-existing workspace lints outside this change (`field_reassign_with_default`,
  `type_complexity`, `too_many_arguments`, `items_after_test_module`). The one
  new `filter_map_bool_then` finding in the proxy iterator was fixed.

## Summary

- Findings remediated: 4 (1 MEDIUM, 3 LOW)
- New findings filed: 0
- Issue mutations: 0
- Remaining hard blockers: packed Havok binary decoding; NIF phantom sensor
  consumption; optional FO3 DLC real-data baseline coverage
