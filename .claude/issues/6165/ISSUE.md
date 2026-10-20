# TD3-2026-10-19-05: CLAUDE.md Workspace Structure tree has three mis-nested entry groups, a stale module list and a missing-directory gap (after the 10-15 rewrite)

Labels: low,documentation,doc-rot,tech-debt
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- Severity: LOW · Effort: trivial (user-owned file: needs the user's edit) · Status: NEW (regression introduced while fixing 10-15 TD3-10)
- Locations: `CLAUDE.md:327-340` (`spt/`), `:355-366` (`debug-ui`/`hkx`/`debug-server`), `:368-374` (`tools/`), `:135-136`
- Evidence (resolved with a path-walk of the tree block against the checkout):
  - Under `spt/` sit `src/parse.rs eval.rs layout.rs raster.rs tex.rs font.rs menu.rs profile.rs` and `examples/render_hud.rs`; none exist in `crates/spt/src` (it has `parser.rs scene.rs stream.rs tag.rs version.rs import/ recon/`). They are `crates/menuxml/src/*` and `crates/menuxml/examples/render_hud.rs`; `menuxml/` (`:319`) has no children listed.
  - Under `hkx/` sit `src/lib.rs`, `listener.rs`, `system.rs`, `evaluator.rs`, `registration.rs` (the DebugDrainSystem / register_component text); those are `crates/debug-server/src/*`. `crates/hkx/src` actually has `animation.rs behavior.rs packfile.rs expression.rs`.
  - Under `texture-upscale/` sit `src/main.rs`, `src/display.rs` (TCP client / REPL); those are `tools/byro-dbg/src/*`.
  - `:135-136` lists 9 domain modules for `components/`; there are 12 (`explicit_start`, `projectile_muzzle`, `game_time` unlisted). Hub is 89 lines (31 production) - "~30 LOC" is fine.
  - `byroredux/src/` directories `boot/` (cli, registries, schedule/*), `streaming/`, `save_io/`, `npc_spawn/`, `scene/`, `systems/` (45 files) are not in the tree; the tree is described as a map, so this is informational.
- Proposed fix: move the three child groups under `menuxml/`, `debug-server/`, `byro-dbg/`; add the three components modules; optionally add `boot/`, `streaming/`, `save_io/` one-liners.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in the neighbouring docs / skills
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
