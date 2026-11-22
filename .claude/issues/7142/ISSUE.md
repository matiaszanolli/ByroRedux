# #7142: PAR-D6-2026-11-20-02: `find_case_insensitive` refuses `..` but not a Windows drive-prefix or `:` component, so a script's `PlayBink` path can leave the data folder on Windows

**Labels**: low,import-pipeline,scripting,safety,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7142

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D6-02 (`/audit-parsers`, HEAD f3713bf5e)

- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Trigger Input**: `PlayBink "C:\somewhere\x.bik"` (or `D:x.bik`, or `x.bik:stream`) from a mod script, on a Windows build
- **Location**: `byroredux/src/systems/script_requests.rs:316-330`, called from `byroredux/src/video.rs:384-396`
- **Status**: NEW
- **Description**: The INC4-3 fix (ba3ee4307) splits on `\`/`/`, drops empty parts, and rejects `.`/`..`. It does not reject a component containing `:`. On Windows `PathBuf::join("C:")` replaces the accumulated path (std: a path with a prefix and no root replaces `self`), so the walk re-roots at the drive's current directory and the later components resolve relative to it, outside `Data`. Linux is unaffected (`:` is an ordinary name character), so no test can show it there. The project already treats this exact class as a hard rule for Steam `installdir` (`is_bare_install_dir` rejects `:` for the same reason, #5016).
- **Evidence**: `find_case_insensitive` only compares `component == ".." || component == "."`; `video.rs::resolve_under` tests cover `..\..\secret.bik` and `Mymod\..\..\secret.bik`, nothing with a prefix. Reasoned from std's documented `Path::join` behaviour; not run on Windows.
- **Impact**: a script can feed any `.bik`-shaped file under the engine's working directory on any drive to the Bink decoder (read only; the decoder rejects non-Bink bytes). It is the same threat INC4-3 closed, narrower and Windows-only.
- **Related**: INC4-3 (ba3ee4307), #5016
- **Suggested Fix**: reject any component whose `Path::new(component).components()` is not a single `Normal` (or simply one containing `:`), and add the Windows-shaped inputs (`C:\x`, `D:x`, `x:y`) to the `resolve_under` test, which is host-independent as a string predicate.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
