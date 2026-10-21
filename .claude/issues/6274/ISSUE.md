# #6274: TD2-2026-10-21-04: exe-relative shipped-asset and cache path resolution is spelled five ways across four crates

**Labels**: low,tech-debt,bug

**Source**: docs/audits/AUDIT_TECH_DEBT_2026-10-21.md · **Severity**: LOW · **Dimension**: 2 · **Location**: `crates/settings-io/src/presets.rs:84-94` (cwd, then exe dir), `crates/game-detect/src/profiles.rs:304-316` and `:403-415` (the same candidate list twice in one file), `crates/renderer/src/vulkan/context/helpers.rs:808-815` (`pipeline_cache_path`, exe dir only), …

- **Severity**: LOW
- **Dimension**: 2
- **Location**: `crates/settings-io/src/presets.rs:84-94` (cwd, then exe dir), `crates/game-detect/src/profiles.rs:304-316` and `:403-415` (the same candidate list twice in one file), `crates/renderer/src/vulkan/context/helpers.rs:808-815` (`pipeline_cache_path`, exe dir only), `byroredux/src/asset_provider/terrain_height.rs:88-98` (`BYRO_CACHE_DIR`, then exe dir, then `.`), `tools/byro-launcher/src/engine.rs:39-42`
- **Status**: NEW (none of the crates was named by the 10-19 report)
- **Age**: the newest copy is `5017a64c2` (2026-10-20); the oldest is #735 (pipeline cache)
- **Effort**: small
- **Description**: The same "resolve next to the executable so a launcher or read-only cwd works" rule is re-derived per subsystem, with three different fallback orders and only one env override. `game-detect` repeats its own two-candidate loop in `load_default_from` and `ordered_config_paths_with`.
- **Evidence**: `grep -rn 'current_exe()' --include='*.rs' crates byroredux tools` returns 6 production sites.
- **Impact**: Packaging rules (TOOL-D4-2026-10-21-02 found the release docs say `assets/` is cwd-only) cannot be stated once. A future portable/read-only install mode has five places to patch.
- **Related**: TOOL-D4-2026-10-21-02, #735 (closed), #5910 (closed, profiles user-path).
- **Suggested Fix**: Add `game_detect::paths::{shipped_asset(rel), cache_dir(sub)}` (or put them in `settings-io`, which the bin and launcher both use) with one documented order (`BYRO_CACHE_DIR` override → exe dir → cwd), and route all five through it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
