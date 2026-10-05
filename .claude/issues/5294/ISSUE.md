# #5294: TOOL-D5-2026-10-05-01: `byro-launcher --profiles` parses loosely, and the engine never reads the file it names

Labels: low,tech-debt,bug
Filed from: docs/audits/AUDIT_TOOLING_2026-10-05.md

**Source**: `docs/audits/AUDIT_TOOLING_2026-10-05.md` (TOOL-D5-2026-10-05-01) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: Install Detection
- **Exposure**: CLI users of the launcher, a documented developer flag (`tools/byro-launcher/README.md:7`). Players launching with no arguments are unaffected.
- **Location**: `tools/byro-launcher/src/main.rs:64-78` (`profiles_path`); `tools/byro-launcher/src/state.rs:72-75,149-158`; `crates/game-detect/src/profiles.rs:273-292,394-398`; `tools/byro-detect/src/main.rs:171-178`
- **Status**: NEW (the launcher sibling of #5167)
- **Description**: The flag has two problems.
  1. **Parsing.** `std::env::args().position(|arg| arg == "--profiles").and_then(|i| std::env::args().nth(i + 1))` has the shape #5167 removed from `byro-detect`:
     - `--profile x` (a typo) and a trailing bare `--profiles` silently select the default `~/.byroredux/profiles.toml`.
     - `--profiles --x` takes `--x` as the path.
     - Unknown arguments are ignored.

     The launcher writes the profiles file on every Play (`remember()` → `merge_into_file`) and writes `boot.toml` beside it, so a typo retargets real writes.
  2. **The engine ignores a custom path.**
     - The launcher reads configured roots from the custom file (`detect_all(&profiles_path)`), validates against them, and records `[roots]` into it.
     - The boot request it hands the engine is a bare profile key (`BootRequest::for_profile`, `state.rs:152`).
     - The engine's loader reads only `home_dir()/.byroredux/profiles.toml` (`profiles.rs:274,288`).

     So a game shown "ready" in a `--profiles` launcher resolves, in the engine, through the default file or `DEFAULT_GAMES_ROOT`. That can be a different root or a missing one. `byro-detect --profiles X --write` has the same gap, while its help text says `--write` "makes `--game <key>` resolve correctly on this machine".

  The per-user path is also computed three times. `game-detect`'s private `home_dir()` returns `None` when `HOME`/`USERPROFILE` are unset, and the engine then skips the file. The launcher's and `byro-detect`'s copies `unwrap_or_default()` to a cwd-relative `.byroredux/profiles.toml`, which the engine never reads.
- **Evidence**: see Location. `docs/engine/launcher.md` has no `--profiles` entry, and the engine has no flag or environment variable that selects a profiles file.
- **Impact**: A developer testing with an alternate profiles file gets a launcher that disagrees with the engine it launches. A typo silently edits the real user file (formatting is now preserved by #5166, so only `[roots]` values change). This is low-likelihood and developer-facing.
- **Related**: #5167, #5166, TOOL-D5-2026-10-02-02
- **Suggested Fix**:
  - Expose one `pub fn user_profiles_path() -> Option<PathBuf>` from `byroredux_game_detect::profiles` and use it in all three places.
  - Parse the launcher's arguments strictly, like `byro-detect`.
  - Either carry the profiles path to the engine (a `BootRequest` field or an environment variable honoured by `load_default`), or document `--profiles` as "detection and validation only; the engine reads `~/.byroredux/profiles.toml`".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
