# #5348 — AUD-2026-10-05-D5-02: Non-body footstep emitters are suppressed whenever PlayerEntity is set, not just the camera's — every future NPC FootstepEmitter would be silent in character mode, and an F-toggled FlyCam stays silent contrary to the comments

- **Labels**: low,audio,bug
- **Filed from**: `docs/audits/AUDIT_AUDIO_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5348

- **Severity**: LOW. It is latent: no production NPC carries a `FootstepEmitter` today. The FlyCam behaviour is arguably desirable but disagrees with the code's own stated intent.
- **Dimension**: Engine Consumers
- **Location**:
  - `byroredux/src/systems/audio.rs:155-158`, `:197-204`: the gate.
  - `systems/character.rs:1063-1083`: `toggle_player_mode` leaves `PlayerEntity` set.
  - `components.rs:1835-1844`: the `FootstepScratch` sizing doc ("5–10 walking NPCs, peak ~50").
  - `scene.rs:977-981`: comment.
  - `late.rs:84-86`: comment.
- **Status**: NEW (introduced by `ccc743160`, #5146)
- **Description**: The new branch is `if let Some(body) = player_body { if entity != body { re-seed; continue } … }`. Two problems follow.
  1. **Opt-in is no longer component-driven.** The skill invariant, the system doc ("Spawn a `FootstepEmitter` on the player entity to opt in") and `FootstepScratch`'s capacity rationale all describe a component that any walker can carry. Under #5146, in character mode, which is the only mode where NPC footsteps matter, every emitter except the player body's is re-seeded and never fires. The #5146 intent was to skip the *camera's* emitter, but the predicate skips everything that is not the body. The first NPC or FOOT (3.5b) consumer that inserts a `FootstepEmitter` will be silently muted, and none of the #5146 tests would notice.
  2. **FlyCam after the F toggle.** `toggle_player_mode` flips `PlayerMode` but leaves `PlayerEntity(Some(body))`, and the body is frozen in FlyCam. The camera emitter therefore stays suppressed and flying is silent. That is arguably right, since flying is not walking. But the code says otherwise in three places:
     - The camera re-seed comment (`:199-201`) says it exists "so a later FlyCam switch doesn't replay the whole boom arc as one stride burst", which assumes the camera resumes stepping after the switch.
     - `late.rs:84-86` says "the emitter lives on the active camera in FlyCam".
     - The `footstep_system` doc says "FlyCam scenes (no body) keep the original camera-is-the-mover behaviour".

     So a `--fly` boot steps while flying and an F-toggled FlyCam does not. Neither behaviour is pinned.
- **Evidence**: see Location. `git grep -n 'FootstepEmitter::new()' -- byroredux/src ':!*/systems/audio.rs'` finds only `scene.rs:982` (the camera) and `scene.rs:1230` (the body). `toggle_player_mode` has no `PlayerEntity` write, and the only production `PlayerEntity` insert is `scene.rs:1223`.
- **Impact**: there is no audible regression today. A latent trap is set for the per-NPC and FOOT footstep work, and the documentation and code disagree on FlyCam.
- **Related**: #5146 (closed), FOOT phase 3.5b, D5-03.
- **Suggested Fix**:
  - Skip only the entity that actually stands in for the player: the `ActiveCamera` entity while `PlayerMode == Character`. Alternatively, remove the camera's `FootstepEmitter` when the body spawns and re-add it on Character → FlyCam if stepping while flying is wanted.
  - Pin whichever FlyCam behaviour is chosen. Add a test in which a third, non-player emitter walking in character mode fires.

_Source: `AUDIT_AUDIO_2026-10-05.md` (AUD-2026-10-05-D5-02), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
