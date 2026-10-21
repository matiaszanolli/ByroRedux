# #6281: AUD-2026-10-21-D3-02: Script sound plays (Papyrus `Sound.Play`, graph `SoundPlay.*`, ObScript `PlaySound`/`PlaySound3D`) put open-ended, script-chosen paths on the unbounded `SoundCache`, decode with no device, and ignore authored attenuation

**Labels**: low,audio,scripting,bug

**Source**: docs/audits/AUDIT_AUDIO_2026-10-21.md · **Severity**: LOW · **Dimension**: Music & SoundCache / Engine Consumers · **Location**: `byroredux/src/systems/scripted_sound.rs:20-70`; `byroredux/src/systems/combat_anim.rs:409-444` (`play_oneshot_cached`); producers `crates/scripting/src/fragment/vm/natives/forms.rs:39`, `fragment/anim_events.rs:146-163`, `obscript_actions.rs:230`, `obscript_effects.rs:953-959`

**Audit status**: NEW (the shape the skill's Dim 3 marks as "the finding"; no issue or audit covers it, and dedup checked #850, #4739 and #5382, all closed)

- **Description**: `scripted_sound_system` resolves any SNDR/SOUN FormID a script names (including multi-`ANAM` variants taken in turn) and plays it through `play_oneshot_cached`, which inserts the decoded `Arc<StaticSoundData>` into the process-lifetime `SoundCache`. That cache has no eviction, and its negative entries are permanent until `clear()`, which nothing calls. Four producers now feed it: Papyrus `Sound.Play`, behaviour-graph `SoundPlay.<edid>` notifies (120 vanilla event names), ObScript `PlaySound` and `PlaySound3D`. Unlike the voice path (#5382, gated on `AudioWorld::is_active` before any archive work), `play_oneshot_cached` extracts and decodes even when the `AudioWorld` is headless and the play is discarded. Its miss log says `"combat sound: '…' not found"` for script sounds. Every script sound also takes combat's hardcoded `{0.5, 24.0}` m window, although FO3/FNV SOUN records author `SNDD` minimum/maximum attenuation distance (×5 and ×100 game units per `wbDefinitionsFNV.pas:7889`) plus `2D` / `Menu Sound` / `Environment Ignored` flags. `SounRecord` carries none of them.
- **Evidence**: `git grep PendingSoundPlays` lists the four producers. `SoundCache` keeps `Option<Arc<_>>` forever. `scripted_sound.rs:66` calls `play_oneshot_cached(world, &path, position)`.
- **Impact**: Over a long Skyrim/FO4 session, every distinct scripted or graph sound stays decoded in RAM, and headless sessions retain them too. Each one is small, but the set is open-ended in the way #5382 fixed for voice. Script sounds play with the wrong range, and 2D/menu sounds play positionally. The log mislabels misses.
- **Related**: #5382 (closed: same issue for voice, fixed with `VoiceSoundCache`); D3-01 (`PlaySound` non-locational). Closed #850 added `SoundCache::clear()` and deferred an LRU "until a real consumer surfaces"; these script producers are that consumer.
- **Suggested Fix**: Route script sounds through a bounded cache (generalize `VoiceSoundCache`'s byte-budget LRU, or give script sounds their own instance) and gate the decode on `AudioWorld::is_active`. Pass the path's role into the miss log. Decode `SNDD` attenuation and flags into `SounRecord` and use them in `scripted_sound_system`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`VoiceSoundCache` and `SoundCache` share one bounded policy; other open-ended-key consumers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
