# #5910: PAR-D6-2026-10-15-02: `load_with_user_path(Some(X))` merges profiles from X but takes `[roots]` from `$BYRO_PROFILES` or the home file

- **Labels**: low,bug,tech-debt
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Location**: `crates/game-detect/src/profiles.rs:336-341`. `if let Some(user_path) = selected_user_path()` ignores the injected `user_path` that `:324-328` honours. Callers: `tools/byro-launcher/src/state.rs:81`, `:97` and `tools/byro-detect/src/main.rs:57`.
- **Status**: NEW (pre-existing; predates the delta, but sits beside the code #5739 moved). #5476 (closed) fixed `[defaults]` and the profile merge, not this overlay.
- **Trigger Input**: `byro-launcher --profiles X` (or `byro-detect --profiles X`) on a machine whose `~/.byroredux/profiles.toml` has different `[roots]` or is malformed. In the malformed case the launcher prints `[roots] could not read ~/.byroredux/profiles.toml: …` for a file it was not pointed at.
- **Description**:
  - `load_with_user_path`'s contract (`:290-294`, #5476) is "the registry they validate against comes from the SAME file they were pointed at". The `[roots]` overlay breaks it.
  - The spawned engine, which gets `BYRO_PROFILES=X`, overlays X's `[roots]`, so the two registries disagree on `entry.root`.
- **Evidence**: code reading. I confirmed no tool currently reads `entry.root` (`grep '\.root\b' tools/byro-{launcher,detect}/src`: no hits). Candidates take `data_dir` from `detect_all(profiles_path)`, which reads X directly. **The defect is latent:** the only live symptom is the stray read and error line.
- **Impact**: latent divergence plus a misleading error line. It would become a wrong-root validation the moment a tool consumes `entry.root`.
- **Related**: #5476, #5294, #5739.
- **Suggested Fix**: overlay `[roots]` from `user_path` when it is `Some`, falling back to `selected_user_path()` only for `load_default`. Pin this with a test like `launch_defaults_follow_the_selected_user_file`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
