# #6276: TD3-2026-10-21-02: new runtime surfaces have no reference doc (loose `Data\` tier, height-map disk cache, env vars)

**Labels**: low,import-pipeline,terrain-exterior,tech-debt,documentation,doc-rot

**Source**: docs/audits/AUDIT_TECH_DEBT_2026-10-21.md · **Severity**: LOW · **Dimension**: 3 · **Location**: `docs/engine/archives.md` (no mention of the loose tier or provider precedence; `grep -c 'loose\|LooseDir\|last-listed' = 0`); `byroredux/src/asset_provider/archive.rs` (#6200 loose tier); `byroredux/src/asset_provider/terrain_height.rs:84-98` (`terrain-height/` cache directory, `BYRO_CACHE_DIR`); …

- **Severity**: LOW
- **Dimension**: 3
- **Location**: `docs/engine/archives.md` (no mention of the loose tier or provider precedence; `grep -c 'loose\|LooseDir\|last-listed' = 0`); `byroredux/src/asset_provider/archive.rs` (#6200 loose tier); `byroredux/src/asset_provider/terrain_height.rs:84-98` (`terrain-height/` cache directory, `BYRO_CACHE_DIR`); `docs/engine/index.md` (the `exal-terrain-height.md` row was dropped as "never written", `e6f74069d`)
- **Status**: NEW
- **Age**: `48e422a71` (2026-10-20, #6200), `5017a64c2` (2026-10-20, #6195)
- **Effort**: small
- **Description**: Loose `Data\` files now outrank every BSA/BA2 (#6200). The "authoritative" archive reference describes only the archive formats, so the precedence rule a modder or the parsers audit needs is undocumented. SF-2026-10-21-D2-01 and PAR-D6-01/02 already found behaviour gaps in that tier. The terrain height maps write a persistent on-disk cache next to the executable, keyed on the source texture hash and `GENERATOR_VERSION`, and no doc mentions where it lives or that `BYRO_CACHE_DIR` moves it. More broadly, 18 of the 59 `BYRO*`/`BYROREDUX_*` env reads appear in no doc, including production switches (`BYROREDUX_PRESENT_MODE` in `vulkan/swapchain.rs:207`, `BYRO_INTERIOR_AMBIENT_SCALE` in `render/mod.rs:572`, `BYRO_RT_FRUSTUM_ONLY` in `render/static_meshes.rs:469`, `BYRO_CACHE_DIR`).
- **Evidence**: the `env::var("BYRO…")` sweep above, cross-checked against `docs/`, `README.md` and `CLAUDE.md`.
- **Impact**: Users cannot clear or relocate the height cache. Audits of the loose tier have no spec to check against. Diagnostic switches are found only by reading code.
- **Related**: PAR-D6-01, PAR-D6-02, SF-2026-10-21-D2-01, PERF-D3-01, #6195/#6196 (open trackers).
- **Suggested Fix**: Add an "Asset provider precedence" section to `archives.md` covering last-listed wins, the loose tier and its case folding. Add a terrain-height paragraph to `exal-groundcover.md` or a short `exal-terrain-height.md` (source tiers, cache dir, `BYRO_CACHE_DIR`, `GENERATOR_VERSION`). Add one env-var table to `debug-cli.md` (or `testing.md` for test-only vars).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
