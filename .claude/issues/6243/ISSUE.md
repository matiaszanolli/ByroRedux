# #6243: PAR-D6-01: The loose `Data\` indexer follows symlinked directories with no depth cap or visited set. A symlink back to an ancestor directory makes the startup walk exponential and hangs boot.

**Labels**: medium,bug,import-pipeline

**Source**: docs/audits/AUDIT_PARSERS_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: I/O & Paths · **Location**: `byroredux/src/asset_provider/archive.rs` (`LooseDir::index`, `LooseDir::walk`)

- **Location**: `byroredux/src/asset_provider/archive.rs:67-97` (`LooseDir::index`, `LooseDir::walk`). Reached from `build_texture_provider` / `build_material_provider` via `Archive::open_loose` (245) at boot.
- **Status**: NEW (in code added by 48e422a71, #6200)
- **Trigger Input**: `Data\textures\a -> ..` and `Data\textures\b -> ..`, two directory symlinks to an ancestor (or one per level of a deeper tree).
- **Description**:
  - `walk` decides "directory" with `path.is_dir()` and "file" with `path.is_file()`, and both follow symlinks.
  - It recurses with no depth limit and no set of visited `(dev, inode)`.
  - On Linux a symlinked-cycle path stops resolving only at `ELOOP`, after 40 symlinks in one path, or at `ENAMETOOLONG`. One looping link therefore re-indexes the subtree about 40 times under ever-longer keys. Two links make that about 2^40 `read_dir` calls.
  - The walk runs once per process, synchronously, before the first cell loads. The registry's caching does not help with the first walk.
  - Windows junctions behave the same way.
- **Evidence**: the code at 81–97 has no guard. None of `asset_provider/tests/loose_override.rs`'s four tests uses a symlink.
- **Impact**:
  - A user whose mod manager deploys by symlink and leaves a looped link hangs at boot.
  - A single loop bloats the index with duplicate keys, all of which then also go through `count_shadowed_entries`.
- **Related**: #6200. The archive readers' bounded table walks (Dim 1).
- **Suggested Fix**:
  - Use `entry.file_type()`, which does not follow symlinks. Either skip symlinked directories or keep a visited set of canonical directory paths.
  - Cap the depth: archive paths in vanilla data are shallow, so 32 levels is generous.
  - Add a symlink-loop fixture under `#[cfg(unix)]`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
