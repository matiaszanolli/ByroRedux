# #3770 — UI-D5-2026-08-30-04: one unresolvable ImportAssets URL in the root movie is a hard menu-load failure, while the identical failure on a nested import and at fetch time is deliberately degraded

**Severity**: MEDIUM · **Location**: `crates/ui/src/navigator.rs`, `crates/ui/src/prepare.rs`, `crates/ui/src/player.rs`
**Source**: `docs/audits/AUDIT_UI_2026-08-30.md` (UI-D5-2026-08-30-04)

`import_asset_paths_from_tags` `.collect()`-ed into `Result<Vec<String>, String>`, so a single
`ImportAssets` URL that failed to resolve (e.g. absolute with a non-`file` scheme —
`movie_url.join` succeeds since it's already absolute, then `archive_path_from_url` rejects the
scheme) aborted the WHOLE root-movie scan and, via `prepare_movie`'s `?`, the whole menu load.
The identical failure is non-fatal both at fetch time (`ScaleformNavigator::fetch` →
`degraded(...)`) and one level down (`load_archive_resource`'s nested-import scan →
`record_degraded(...)`) — `from_resource_provider`'s own comment states the policy the root scan
violated: "a dependency that fails to fetch is recorded, not fatal."

## Fix implemented

- `import_asset_paths_from_tags` now **partitions rather than short-circuits**: returns
  `(Vec<String> resolved_paths, Vec<String> error_messages)` instead of
  `Result<Vec<String>, String>`. One bad URL no longer costs every other resolvable sibling in
  the same scan.
- `import_asset_paths` (the depth-≥1 byte-taking wrapper) keeps `Result` only for its own
  genuinely-fatal decompress/parse failures (no tag list to partition at all) — its `Ok` payload
  now carries the same `(paths, errors)` pair. `load_archive_resource` extends
  `NavigatorState.errors` with the per-URL failures instead of losing them inside a whole-resource
  degrade.
- `PreparedMovie` gained `root_import_errors: Vec<String>`; `prepare_movie`'s root scan no longer
  `?`-propagates a partial failure.
- `ScaleformNavigatorRuntime::create` gained an `import_errors: Vec<String>` parameter, pushed
  straight into the freshly-constructed `NavigatorState.errors` — so a root-movie import failure
  surfaces through `SwfPlayer::resource_errors()` exactly the same way a fetch-time or
  nested-import failure already did, instead of aborting construction entirely.

Regression test (issue's own TESTS checklist item):
`root_movie_with_one_unresolvable_import_still_loads` — a root movie with one resolvable and one
unresolvable (absolute, non-`file`-scheme) `ImportAssets` URL; asserts the load succeeds, the
resolvable sibling still fetches normally, and the failure is recorded in `resource_errors()`.
Traced the pre-fix code path by hand (the removed `.collect()` → `?` → `anyhow!` chain) to
confirm the test would have failed there; the fix and test landed in the same commit so an
isolated stash-based re-run wasn't practical without also reverting the test.

**SIBLING** (issue's own checklist item): #3428 (the `avm2_host.rs` lifecycle-class-scan twin
this issue named) is already CLOSED, fixed separately. Grepped every `.collect()` in
`crates/ui/src/` for another `Result<Vec<_>, _>`-typed root-vs-nested asymmetry — none found; the
remaining collects in `avm2_host.rs`/`host.rs` all target infallible `Vec<T>`, which can't
exhibit this failure shape at all.

Full workspace: `cargo test --no-fail-fast` 7043 passing, 0 failing.
