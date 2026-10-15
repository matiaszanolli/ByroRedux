# #5876: GAME-D2-2026-10-15-05: Doc and tracking drift: the `PapyrusPlayerEntity::get` sync claim, and Skyrim `.fuz` voice

- **Labels**: low,documentation,doc-rot,dialogue
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-15.md`

- **Location**: `crates/scripting/src/papyrus_demo/mod.rs:75-84`; ROADMAP M43; `byroredux/src/systems/dialogue_voice.rs`
- **Status**: NEW (doc-rot)
- **Description**:
  - The `get` doc says `extension_player_entity_sync_system` keeps `PlayerEntity` in sync. That system only copies `PlayerEntity` into the WASM host. The two resources agree because `scene.rs:1195/1217` writes both to the same body; in FlyCam they differ, harmlessly.
  - Skyrim `.fuz` voice is still unbuilt: #5464 corrected only the container description, and `dialogue_voice.rs` resolves `.ogg` only. ROADMAP points the gap at closed #5367, and no open issue tracks it.
- **Suggested Fix**: Correct the doc comment. File, or link, an open tracker for the xWMA `.fuz` decoder.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **LOCK_ORDER**: If a RwLock/query scope changes, TypeId-sorted acquisition and the system's declared `Access` row are preserved
- [ ] **TESTS**: A regression test pins this specific fix
