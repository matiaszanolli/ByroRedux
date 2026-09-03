# #2584 — SK-D5-LZ4-LOW-01: open_with_numeric_siblings has no de-dup guard against explicitly re-listing an auto-loaded sibling

**Severity**: LOW · **Dimension**: BSA v105 (LZ4)
**Location**: `byroredux/src/asset_provider/archive.rs::open_with_numeric_siblings`, `byroredux/src/asset_provider/texture.rs::build_texture_provider`

## Fix

Applied the issue's own suggested fix: added a `HashSet<String>` of
already-opened (ASCII-lowercased) archive paths, checked before both the
primary open and each sibling open in `open_with_numeric_siblings`.
`build_texture_provider` owns one set per pool (`mesh_opened` /
`textures_opened`, matching the existing separate `mesh_archives` /
`texture_archives` Vecs), reused across every `--bsa`/`--textures-bsa`
occurrence in one run — so a sibling auto-loaded from an earlier archive
is correctly recognised if the user also lists it explicitly later,
regardless of argument order.

Case-insensitive comparison (lowercase before insert/check), matching
Bethesda's own filesystem case-insensitivity convention this codebase
already follows elsewhere (`numeric_sibling_paths`'s own extension match,
`find_by_basename`, etc.) — a user typing a different case than the
auto-computed sibling path still de-dups correctly.

Extracted the decision itself into a pure helper, `mark_opened(path,
opened_paths) -> bool`, so it's directly unit-testable without needing
real archive files on disk — `open_with_numeric_siblings` needs a genuine
BSA/BA2 file to reach its `Archive::open` calls, but the de-dup decision
doesn't depend on that at all.

## TESTS (issue's own checklist item)

`explicitly_relisted_auto_loaded_sibling_is_recognised_as_already_opened`
— the exact scenario the issue describes: `Meshes0.bsa` opens, its
sibling `Meshes1.bsa` auto-loads, then the user's own explicit re-list of
`Meshes1.bsa` arrives — the third call must report "already opened."
Plus `mark_opened_is_case_insensitive` and
`distinct_paths_are_independent` (mid-series digits, which
`numeric_sibling_paths` deliberately never auto-expands, must never
collide with each other in the de-dup set).

**Reintroduce-and-revert verification**: temporarily changed `mark_opened`
to skip the lowercase fold (`path.to_string()` instead of
`path.to_ascii_lowercase()`) — confirmed
`mark_opened_is_case_insensitive` failed with the expected message.
Restored the fix and reran — all 5 tests in
`asset_provider::archive::tests` pass again.

## Verification

- `cargo check -p byroredux --tests`: clean, zero warnings.
- `cargo test -p byroredux asset_provider::archive::tests::`: 5 tests
  passing, 0 failing (+3 new).
- `cargo test -q -p byroredux`: passing.
- `cargo test -q --no-fail-fast` (full workspace): **7156 passing, 0
  failing**.
