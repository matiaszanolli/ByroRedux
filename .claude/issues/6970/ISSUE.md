# #6970: TD3-2026-11-15-05: `music.md` names two types that do not exist (`MusicDirective`, `PendingMusicEvents`)

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6970
- **Labels**: low,audio,documentation,doc-rot,tech-debt
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW. **Dimension**: 3. **Effort**: trivial. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none (00472c4a1 / #6934 swept the music commits' doc rot but missed these).
- **Evidence**: `docs/engine/music.md:15` ("become a `MusicDirective`") and `:43` ("a level-up (`PendingMusicEvents`, pushed where the level is awarded)"). Code: the type is `Directive` (`byroredux/src/music/directive.rs:75`); level-up stings go through `raise_sting` into `MusicCatalog::stings` (`byroredux/src/music/mod.rs:94,155-160`). The gate listed both as unresolved symbols.
- **Fix**: `Directive`; `raise_sting` (`MusicCatalog.stings`).
- **Advisory triage, other window-rename hits**: `docs/engine/exterior-readiness-plan.md:562,1013,1228-1265` still name `apply_cell_climate_override` (replaced by `apply_cell_weather`, ea5417dc7) and `AudioWorld::play_music`/`stop_music` (now `play_music_voice`, `crates/audio/src/world.rs:553`) as live; the plan is a dated narrative, so italicise rather than rewrite.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
