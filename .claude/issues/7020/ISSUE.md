# #7020 — PHYS-D4-01: `probe_spawn_ground`'s rustdoc opens with half of `yaw_pitch_from_forward`'s doc

**Labels**: low,physics,documentation,doc-rot · **Filed from**: `docs/audits/AUDIT_PHYSICS_2026-11-15.md` (2026-11-15)

- **Severity**: LOW
- **Dimension**: Character & NPC Controller
- **Location**: `byroredux/src/scene/character_spawn.rs:255-273`; the other half at `byroredux/src/scene.rs:148-151`
- **Status**: NEW
- **Description**: The file split in e9aeb2706 (2026-09-21) cut the rustdoc of `yaw_pitch_from_forward` in two. Its head ("Decompose a forward vector into the `(yaw, pitch)` pair that `fly_camera_system` would need… so `pitch = asin(y)` and `yaw = atan2(-x, -z)`…") now sits directly above "Probe for walkable ground beneath `cam_pos`" with no blank line, so it is part of `probe_spawn_ground`'s doc. The function it describes lives in `scene.rs:153`, whose doc now starts mid-thought at "Looking exactly along ±Y is the degenerate case".
- **Evidence**: `git blame -L 255,273 byroredux/src/scene/character_spawn.rs` → all e9aeb2706; `git grep -n 'fn yaw_pitch_from_forward'` → `byroredux/src/scene.rs:153` only.
- **Trigger Conditions**: none (documentation).
- **Impact**: A reader of the player-spawn ground probe (the Character-vs-FlyCam gate, #2375/#3002) is told it decomposes a camera vector; the real decomposition's #2383 rationale (why not a shortest-arc quaternion) is missing where it applies.
- **Related**: #2383, #6814 (last edit to this function).
- **Suggested Fix**: Move lines 255-269 back above the "Looking exactly along ±Y" paragraph on `yaw_pitch_from_forward` in `scene.rs`.

Source: `docs/audits/AUDIT_PHYSICS_2026-11-15.md`

## Completeness Checks
- [ ] **SIBLING**: Every other statement of the same rule (docs, module docs, skill text) checked
- [ ] **TESTS**: n/a (documentation) — or the doc's claim is pinned where a guard already exists
