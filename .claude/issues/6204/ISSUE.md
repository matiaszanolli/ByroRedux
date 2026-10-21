# #6204 — INC-2026-10-20-03: `terrain.blend` claims a session-only change "without saving it", but the next unrelated settings save persists it

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6204
- **Labels**: low,tech-debt,bug
- **Filed**: 2026-10-20 by /audit-publish

From `docs/audits/AUDIT_INCREMENTAL_2026-10-20.md` (`/audit-suite --preset quick`, HEAD c4df4c91e).

- **Severity**: LOW
- **Dimension**: tooling / ecs (settings contract)
- **Location**: `byroredux/src/commands/terrain.rs:89-92` (doc), `:113-117` (`world_resource_set::<SettingsRegistry>` → `registry.set`); `byroredux/src/main.rs:1754-1757`; `crates/settings-io/src/lib.rs:268-273`
- **Changed in**: `byroredux/src/commands/terrain.rs` (commit ea45af067)
- **Status**: NEW (related class: #4975, closed: registry/live-state divergence persisting an unintended value)
- **Description**: The command writes the live `SettingsRegistry` directly. `settings_io::save` writes every entry whose value differs from its last-synced value (not only the entry that changed), and any later settings-menu change triggers that save. The console-set band therefore lands in `settings.toml` at the next unrelated settings save, contrary to the doc comment and the `debug-cli.md:478` "for this session" wording.
- **Evidence**: `save_to_path`: `for entry in registry.entries() { if pinned.contains(&entry.id) || synced.get(&entry.id) == Some(&entry.value) { continue; } settings.insert(...) }`.
- **Impact**: An A/B experiment value (e.g. `terrain.blend 0`) can silently become the user's persisted default.
- **Suggested Fix**: Either drop the "without saving" claim (and say it persists with the next settings save), or make the console path session-only, e.g. a separate runtime override resource read by `app_frame.rs`, or pin the id in `SettingsPersistence` for the session.

## Completeness Checks
- [ ] **LOCK_ORDER**: If the console path gains a separate override resource, TypeId-sorted acquisition is preserved alongside the `SettingsRegistry` write
- [ ] **SIBLING**: Other console commands that write `SettingsRegistry` directly checked for the same persist-on-next-save behaviour
- [ ] **TESTS**: A regression test pins whether a console-set value is (or is not) written by the next unrelated `settings_io::save`
