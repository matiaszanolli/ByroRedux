# #7141: PAR-D6-2026-11-20-01: The loose walker's directory budget does not bound indexed entries, so a link fan-out multiplies keys and stat calls

**Labels**: low,import-pipeline,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7141

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D6-01 (`/audit-parsers`, HEAD f3713bf5e)

- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Trigger Input**: a `Data\textures` (or `meshes`/`materials`/`music`) tree where a chain of directories each carry F files and link twice to the next (the `loose_tier_bounds_a_chain_of_doubled_links` shape, depth 18, F >> 1)
- **Location**: `byroredux/src/asset_provider/archive.rs:59-66` (`MAX_LOOSE_DIRS`), `:164-215` (`walk`)
- **Status**: NEW (#6483 closed; this is the residual of its own bound)
- **Description**: After #6483 each directory is walked once per distinct path to it, not once per canonical path, and only the count of directories (262,144) is capped. Each walked directory then inserts all its files under a distinct key and does two stats per entry (`is_dir`, `is_file`). Keys and work are therefore bounded by 262,144 x F, not by 262,144. The test pins the directory bound with one file per level, which hides the multiplier.
- **Evidence**: `state.walked >= state.budget` is the only work gate; `files.entry(child)` inserts per file with a path-qualified key (`textures\root\l0\l1\...\f.dds`, all distinct). At F = 100 the budget admits ~26 M entries (several GB of `String` + `PathBuf`) and ~50 M stat calls at boot, before the one warn line. A real mod-manager layout (one mod directory linked into a handful of places) is nowhere near this.
- **Impact**: boot stall or OOM on a hostile or accidentally cyclic-by-fan-out link tree; needs local filesystem control (a mod archive extracted with symlinks), so hardening only. When the budget does trip, the surviving subset is whichever branches `read_dir` yielded first, so the partial index differs run to run.
- **Related**: #6243, #6483
- **Suggested Fix**: also cap total indexed entries (or cap re-walks per canonical directory, e.g. 8) and warn once on either; sort `read_dir` entries so a truncated index is deterministic.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
