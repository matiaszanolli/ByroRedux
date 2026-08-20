# Issue #3156: UI-D5-03: NavigatorState.import_asset_paths is the one movie-keyed set in crates/ui that #2964/#2967 left unbounded

- **Finding ID**: `UI-D5-03`
- **Severity**: LOW
- **Labels**: `low,tech-debt,bug`
- **Source report**: `docs/audits/AUDIT_UI_2026-08-20.md`
- **Filed**: 2026-08-20 (comprehensive 25-audit sweep, `/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3156

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3156 --json state`.

---

- **Severity**: LOW
- **Dimension**: 5 — Resource Navigator
- **Location**: `crates/ui/src/navigator.rs`:92 (the field), :106-111 (the seed), :247, :257-258 (the extend)
- **Status**: NEW

## Description

`NavigatorState.import_asset_paths: HashSet<String>` is seeded from the root
movie's `ImportAssets` tags (`navigator.rs`:110-111) and extended from every
imported movie's own tags inside `load_archive_resource` (`:257-258`). Keys are
archive paths chosen by movie content.

It has **no cap, no dedup ceiling and no clear** — the only movie-keyed channel
in the crate without one after `MAX_QUEUED_CALLS` (#2714),
`MAX_DISTINCT_HOST_METHOD_NAMES` (#2964), `MAX_RECORDED_RESOURCE_ERRORS` (#2720)
and `MAX_RECORDED_RESOURCE_LOADS` (#2967).

## Evidence

```
$ grep -n "import_asset_paths" crates/ui/src/navigator.rs
92:     import_asset_paths: HashSet<String>,
110:        .import_asset_paths
111:        .extend(import_asset_paths(&movie_url, movie_data)?);
247:    let is_import_asset = state.borrow().import_asset_paths.contains(&archive_path);
258:        Ok(paths) => state.borrow_mut().import_asset_paths.extend(paths),
476: fn import_asset_paths(movie_url: &Url, movie_data: &[u8]) -> ...
```

No `insert_bounded`, no `MAX_*` const, no `.clear()`.

## Impact

**Bounded in practice** — growth requires distinct paths that the provider
actually resolves, so the ceiling is the archive's real import graph (a handful
of entries for vanilla content). Reported for consistency with the crate's own
stated policy (`host.rs`:48-53: *"a cap that never engages costs nothing, and one
that's needed and missing is a slow OOM"*), **not** because a leak is reachable
today.

Note this path currently has no engine caller at all (#3147), so the practical
exposure is zero until that lands — which is also the moment the cap would start
to matter.

## Related

- #2964, #2967, #2720 — the three caps that established the policy this field
  does not follow
- #3147 (UI-D5-02) — the path is unreachable from the engine today

## Suggested Fix

Reuse the `insert_bounded` shape from `crates/ui/src/host.rs`:200-217 with its
own const.

---
**Source**: `docs/audits/AUDIT_UI_2026-08-20.md` (finding `UI-D5-03`)

## Completeness Checks
- [ ] **SIBLING**: Sweep `crates/ui` once more for any remaining movie-content-keyed collection without a cap
- [ ] **TESTS**: A regression test pins this specific fix — cap engages, latched warn fires once
