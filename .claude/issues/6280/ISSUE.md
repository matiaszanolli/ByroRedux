# #6280: TOOL-D4-2026-10-21-02: The release README and `docs/releasing.md` say `assets/` is read only from the current directory, but both loaders fall back to the executable's directory

**Labels**: low,tech-debt,documentation,doc-rot

**Source**: docs/audits/AUDIT_TOOLING_2026-10-21.md · **Severity**: LOW · **Dimension**: Boot Handoff & Persistence (release packaging docs) · **Location**: `packaging/linux/README.txt:15-16`, `:31-32` and `:45-47` ("REQUIRED, read relative to the current directory"; "the engine and launcher look for assets/ in the current directory"; and the NOTE under "Running": "byroredux cannot find assets/debug_profiles.toml unless your current directory contains …

- **Severity**: LOW
- **Dimension**: Boot Handoff & Persistence (release packaging docs)
- **Exposure**: players using the alpha tarball
- **Location**: `packaging/linux/README.txt:15-16`, `:31-32` and `:45-47` ("REQUIRED, read relative to the current directory"; "the engine and launcher look for assets/ in the current directory"; and the NOTE under "Running": "byroredux cannot find assets/debug_profiles.toml unless your current directory contains an assets/ folder. Copy or symlink this folder's assets/ ..."), `docs/releasing.md:19` ("Read **relative to the current directory**, so the binaries are run from this folder"). Both were added by `13877d3ce`.
- **Status**: NEW
- **Description**: `crates/game-detect/src/profiles.rs` `load_default_from` and `ordered_config_paths_with` try `assets/debug_profiles.toml` relative to the current directory first, then `current_exe().parent()/assets/...` (since `85c22964a`, 2026-08-30; the code comment says "binary-parent-relative (release ships)"). `crates/settings-io/src/presets.rs` `PresetFile::load_default` does the same for `graphics_presets.toml`, and its doc says it does this "so a release build beside its `assets/` works without a working-directory convention". The tarball puts `byroredux` beside `assets/`, so the documented workflow (run from the game's `Data` directory) finds both files with no copy or symlink. On Linux `current_exe` resolves symlinks, so a symlink to the binary in `~/bin` works too.
- **Evidence**: `profiles.rs:308-321` and `:403-416`; `presets.rs:81-95`.
- **Impact**: players are told to copy or symlink `assets/` into each game's Data directory. A copy taken that way wins over the shipped file (the cwd copy is tried first), and it goes stale on the next release, so following the instruction produces old profiles or presets.
- **Related**: `13877d3ce`, #5294
- **Suggested Fix**: Say the binaries find `assets/` beside themselves, and that an `assets/` in the current directory overrides it. Drop the "copy or symlink" NOTE.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] Packaging smoke: the tarball layout run from a game `Data` directory finds both `assets/` files with no copy

_Source: `docs/audits/AUDIT_TOOLING_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
