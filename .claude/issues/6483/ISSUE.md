# PAR-D6-01: The loose walker's global canonical visited set reaches a directory linked from two places under only one of the two keys, chosen by `read_dir` order, with no warning

Labels: low, import-pipeline, bug
Source: docs/audits/AUDIT_PARSERS_2026-10-27.md

**Source**: `docs/audits/AUDIT_PARSERS_2026-10-27.md` finding `PAR-D6-01` (LOW)
**Location (re-mapped to current tree)**: `byroredux/src/asset_provider/archive.rs` `LooseDir::walk` (`visited.insert(canonical)`, ~:119-124); code added by 120b7df86 (#6243)

**Severity**: LOW. Dimension: I/O & Paths.

## Description
The visited set is global to the index rather than an ancestor stack, so a cycle and a second legitimate link look the same. Example: `Data/textures/a -> /mods/x` and `Data/textures/b -> /mods/x`. The files of `/mods/x` are indexed under `textures\a\...` or `textures\b\...`, whichever `read_dir` yields first; the other key silently falls back to the archive copy. The game itself resolves both paths. #6308 made same-key *file* collisions deterministic and warned once; this directory-level drop is neither.

## Evidence
`LooseDir::walk` does `if !visited.insert(canonical) { return; }` before `read_dir` (verified at HEAD); iteration order is `read_dir` (filesystem-dependent).

## Impact
Loose-override content reachable via a second symlinked path is nondeterministically ignored with no log line. Rare layout (symlinked mod dirs), hence LOW.

## Related
#6243, #6308.

## Suggested Fix
Cut cycles with an ancestor stack of canonical paths and bound total work with an entry budget (which is what #6243 needed the global set for); alternatively keep the set but record the second link's key as an alias and warn once.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling systems
- [ ] **TESTS**: A regression test pins this specific fix

