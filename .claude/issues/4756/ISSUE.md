# TOOL-D2-2026-09-22-02: docs/engine/debug-cli.md's registration counts are stale, and 15 of 92 console commands have no documentation row

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4756

## Description
`docs/engine/debug-cli.md` carries two stale counts and 15 undocumented commands, confirmed at HEAD `c3f298a24`:
- Line 179 ("Currently registered (23 components)"), also repeated at lines 950 and 1155, vs. live `grep -c 'register_component::<' crates/debug-server/src/registration.rs` = **42**.
- Line 276 ("Current registered commands (**68**)") vs. live `grep -c 'registry.register(' byroredux/src/commands/mod.rs` = **92**.

Cross-referencing all 92 command names against the full text of the doc (not just the summary block) finds 15 with zero mention anywhere in the file — spot-checked 10 by exact `fn name(&self)` string against `grep -n <name> docs/engine/debug-cli.md`, all confirmed zero hits (`hardcore`, `exposure`, `tonemap`, `depth.stats`, `npc.appearance`, `ragdoll.status`, `sdk.compat`, `rt.masks`, `cell.owners`, `quest.effects`): the entire HUD family (`hud.on`, `hud.off`, `hud.values`, `hud.heading`, `hud.status`, `hud.debug`) plus those 9 (report lists a 15th, `hardcore`, already included above). `exposure` returns 5 hits in the doc, but all 5 are unrelated prose about the rendering concept ("regardless of exposure"), not a documented command row — confirmed by reading each hit. No documented command is missing from the registry (drift is one-directional).

## Evidence
```
$ grep -c 'register_component::<' crates/debug-server/src/registration.rs
42   (doc says 23, debug-cli.md:179,950,1155)
$ grep -c 'registry.register(' byroredux/src/commands/mod.rs
92   (doc says 68, debug-cli.md:276)
$ grep -n 'exposure' docs/engine/debug-cli.md
995: ...regardless of exposure)...   (prose, not a command row)
1044,1046,1049,1063: ...           (all prose)
```

## Impact
Discoverability only — `help` lists everything live — but a developer reading the doc to learn the surface will not find these 15 commands or the correct component-registry size.

## Related
None found. Similar doc-drift pattern to `TD4-2026-08-27-05`, `TD4-2026-09-05-01`, `SCR-ORCH-2026-09-06-01` in other docs this audit cycle (per the source report's own cross-reference), but this is a distinct file/finding.

## Suggested Fix
Regenerate both counts and add rows for the 15 missing commands; consider a doc-drift CI guard that asserts the doc mentions every `CommandRegistry`/`ComponentRegistry` name.

Source: docs/audits/AUDIT_TOOLING_2026-09-22.md (TOOL-D2-2026-09-22-02)

## Completeness Checks
- [ ] **TESTS**: A doc-drift guard test (or CI check) asserts the doc's counts match `grep -c` against the live registries, so this doesn't silently re-drift
