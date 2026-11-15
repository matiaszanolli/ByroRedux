# #6969: TD3-2026-11-15-04: `dependencies.md` "Internal Crate Dependencies" table lists 24 of 33 crates and gets five rows wrong; `architecture.md` says 35 members and omits `video`, `bink2`, `binkaudio`

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6969
- **Labels**: low,dependencies,documentation,doc-rot,tech-debt
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW. **Dimension**: 3. **Effort**: small. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none open. Recurrence of closed #6164 ("27 members, it is 34") and #5599.
- **Evidence** (`[dependencies]` of each `crates/*/Cargo.toml` vs the doc):
  - Missing from the table entirely: `bink2` (leaf), `binkaudio` (leaf), `video` (-> bink2, binkaudio), `boot-request` (-> core), `game-detect` (-> bsa, core, plugin), `menuxml` (leaf), `sdk` (-> core), `settings-io` (-> core), `terrain-height` (leaf).
  - Wrong rows (`docs/engine/dependencies.md:137-170`): `byroredux-plugin` (doc: core; actual core, sdk, bsa), `byroredux-scripting` (doc omits save, sdk, hkx), `byroredux-debug-server` (doc omits scripting), `byroredux-debug-ui` (doc: core; actual core, sdk), `byroredux-mod-runtime` (doc: "none, leaf"; actual -> sdk).
  - `docs/engine/architecture.md:17,24` "35 members (2026-10-22)": 38 today (ROADMAP Project Stats is right); the tree at `architecture.md:26+` has no `video`, `bink2`, `binkaudio` (added 2026-11-11..13: 6e64c453b, 8e11d49a4, 3ebecb7ec). README, `workspace-map.md` and `index.md` do list them.
- **Fix**: regenerate the table from `cargo metadata` (or `rg '^byroredux' crates/*/Cargo.toml`) and add a source-scan test beside `workspace_hygiene_tests.rs` that every `crates/*` name appears in the table, the way `every_registered_system_is_named_in_the_live_schedule_table` does for the schedule. Replace the hand-typed member count in architecture.md with a pointer to ROADMAP Project Stats (it already says "live count is in ROADMAP").

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
