# #5858: GAME-D4-2026-10-15-04: An infinite `Bleedout` timer serializes as JSON `null`, so every save taken afterwards fails to load

- **Labels**: high,bug,gameplay,save-load,combat,game:starfield
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-15.md`

- **Severity**: HIGH (save severity table: a load that refuses gameplay state; every later save in the session is affected)
- **Dimension**: Combat & Death (Dim 4), with Dim 7 (save coverage)
- **Location**: `crates/scripting/src/bleedout.rs:211-213`; save registration `byroredux/src/save_io/registry.rs:372`
- **Status**: NEW (from `dd843f9e6`)
- **Trigger**: a game with no `BleedoutRules` installed (Starfield, FO76): script `Kill()` on an Essential actor, or on a Protected actor whose killer is not the player. This includes Essential/Protected quest aliases. Then save, then load.
- **Description**: When no `BleedoutRules` resource exists, `spare_from_death` stores `Bleedout { remaining_secs: f32::INFINITY }`. `bleedout_system` returns early without `BleedoutRules`, so the actor never recovers and the component stays for the rest of the session. The save registry serializes components through `serde_json::to_value`, which writes non-finite floats as `null`. On load, `validate_snapshot_types` rejects the row ("null, expected f32"; `crates/save/src/driver.rs:122`, `byroredux/src/save_io/live_load.rs:433`).
- **Evidence**:
  ```rust
  let remaining_secs = world
      .try_resource::<BleedoutRules>()
      .map_or(f32::INFINITY, |rules| rules.death_time_secs);
  …
  bleedouts.insert(victim, Bleedout { remaining_secs });
  ```
- **Impact**: Every save from that point on cannot be loaded. On Starfield/FO76 NPCs have no `ActorVitals`, so the reach is limited to `scripted_kill_system`, which does not need them; the impact is total loss of save progress.
- **Suggested Fix**: Use a finite sentinel, or `Option<f32>` with "no rules" meaning "never recovers". Alternatively, decline to spare when no `BleedoutRules` exist. Add a round-trip test for `Bleedout` with the no-rules value.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **LOCK_ORDER**: If a RwLock/query scope changes, TypeId-sorted acquisition and the system's declared `Access` row are preserved
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **SAVE**: Any serialized-shape change bumps `FORMAT_MAJOR`, adds no serde default (#4465), and refreshes the shape baseline (`/audit-save`)
