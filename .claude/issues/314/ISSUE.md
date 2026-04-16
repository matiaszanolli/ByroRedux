# Issue #314 — D6-01: Stale pub(crate) constructor doc comments reference raw track_read/track_write

- **Severity**: LOW
- **Dimension**: Query Safety (documentation)
- **Source**: `docs/audits/AUDIT_ECS_2026-04-14.md`
- **Created**: 2026-04-14
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/314

## Locations
- `crates/core/src/ecs/query.rs:30-31` (`QueryRead::new`)
- `crates/core/src/ecs/query.rs:83-84` (`QueryWrite::new`)
- `crates/core/src/ecs/query.rs:201-204` (`ComponentRef::new`)
- `crates/core/src/ecs/resource.rs:24-25` (`ResourceRead::new`)
- `crates/core/src/ecs/resource.rs:68-69` (`ResourceWrite::new`)

## Summary
Doc comments tell callers to pre-call `lock_tracker::track_read`/`track_write`, but
post-#137 every `world.rs` call site uses the RAII `TrackedRead`/`TrackedWrite` scope
guards + `defuse()`. `ComponentRef::new` doc additionally directs callers to manually
`untrack_read` on the `None` branch — following it would double-untrack (see
`World::get` at `world.rs:122-140` for the correct pattern).

## Suggested Fix
Replace the five stale comments to describe the RAII contract; remove the
manual-untrack sentence from `ComponentRef::new`.
