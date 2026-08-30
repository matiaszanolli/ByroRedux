# #3770 — UI-D5-2026-08-30-04: one unresolvable ImportAssets URL in the root movie is a hard menu-load failure, while the identical failure on a nested import and at fetch time is deliberately degraded

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: medium, ui, bug

---

**Audit**: `/audit-ui` — `docs/audits/AUDIT_UI_2026-08-30.md` (Dimension 5 — Resource Navigator), HEAD `64f64480`
**Finding ID**: `UI-D5-2026-08-30-04`

- **Severity**: MEDIUM
- **Status**: NEW
- **Profile**: both (archive route)

## Location

- `crates/ui/src/navigator.rs:517-535` — `import_asset_paths_from_tags`
- `crates/ui/src/prepare.rs:91` — the `Err` return
- `crates/ui/src/player.rs:224-225` — `SwfPlayer::from_resource_provider` maps it to `anyhow!`
- `byroredux/src/scene.rs:1580-1584` — the terminal log site

## Description

`import_asset_paths_from_tags` ends in `.collect()` into a `Result<Vec<String>, String>`, so **a single** `ImportAssets` entry whose URL fails to `join` or fails `archive_path_from_url` aborts the whole list:

```rust
tags.iter()
    .filter_map(|tag| match tag {
        Tag::ImportAssets { url, .. } => Some(url.to_string_lossy(swf::UTF_8)),
        _ => None,
    })
    .map(|relative| {
        movie_url.join(&relative.replace('\\', "/"))
            .map_err(|error| format!("invalid ImportAssets URL {relative:?} in {movie_url}: {error}"))
            .and_then(|url| archive_path_from_url(&url))
    })
    .collect()
```

On the archive route that `Err` is returned by `prepare_movie`, mapped to `anyhow!` in `SwfPlayer::from_resource_provider`, and logged by `scene.rs` as `Failed to load archive menu …` — **no menu at all**.

The same two failures are non-fatal everywhere else in this module:

- **At fetch time**, `ScaleformNavigator::fetch` routes both a `resolve_url` error and an `archive_path_from_url` error through `self.degraded(...)` (`navigator.rs:340-356`), recording the message and substituting a valid empty movie so Ruffle's `awaiting_import` flag clears.
- **One level down**, `load_archive_resource` calls the *same* `import_asset_paths` on a fetched child's tags and maps its `Err` to `record_degraded` (`navigator.rs:280-283`) — non-fatal.

So the identical error is fatal at depth 0 and recoverable at depth ≥ 1, and `from_resource_provider`'s own comment states the policy the root scan violates:

> "#2720 — a dependency that fails to fetch is recorded, not fatal: the root movie loaded, and a menu missing an imported font is worth more than no menu."

## Reachable input

The concrete trigger is an `ImportAssets` URL that is absolute with a non-`file` scheme — `movie_url.join("http://host/x.swf")` succeeds, then `archive_path_from_url` rejects the scheme (`navigator.rs:465-470`). `ImportAssets` URLs are chosen by untrusted movie content, and R4's stated premise is preserving third-party SWF mod compatibility, so this is not a purely hypothetical shape.

The audit could not sweep vanilla content for such a URL (no FO4/Skyrim corpus on that machine), which is why this is MEDIUM and not HIGH.

Re-verified at HEAD: `navigator.rs:517-535` unchanged, still `.collect()`.

## Impact

One unresolvable import URL in a root movie is the difference between a degraded menu and **no menu**, on a code path whose own comment declares the opposite policy. It is reachable from untrusted third-party SWF content, which is the compatibility surface R4 exists for.

## Related

- **#3428** — structurally the *same asymmetry* (fatal lifecycle-class scan vs non-fatal inventory scan over the same ABC tags in `avm2_host.rs:189-190` / `:210-220`). Not a duplicate; worth fixing in one change.
- #2720 (the policy comment this violates)

## Suggested Fix

Mirror the depth-≥1 policy — **partition rather than short-circuit**. Keep the resolvable paths and push each unresolvable one into `NavigatorState.errors` so it surfaces through `resource_errors()`.

The set's **only** consumer is the `is_import_asset` test at `navigator.rs:270`, which decides whether `prepare_import_asset_swf`'s frame-boundary workaround applies to a fetched resource; an entry that can never resolve to an archive path can never be fetched from the archive either, so dropping it from the set costs nothing.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — specifically `avm2_host.rs`'s #3428 twin, and any other root-vs-nested `collect::<Result<..>>` in `crates/ui/src/`
- [ ] **TESTS**: A regression test pins this specific fix — a root movie with one unresolvable `ImportAssets` URL must still load, with the failure recorded in `resource_errors()`
