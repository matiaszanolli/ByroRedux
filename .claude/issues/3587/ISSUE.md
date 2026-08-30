# #3587 — REN-2026-08-30-D5-04: `rebuild_geometry_ssbo_inner`'s gate comment says a first build takes the chunked path; it takes the atomic one

**Labels**: `low,renderer,memory,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3587 --json state`.

---

- **Severity**: Low
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/mesh.rs:1370-1372` (comment) vs `:1386,1435` (code)
- **Status**: Open — introduced alongside the #3443 fix in `fa511bbf`.
- **Description**: The comment immediately above the #3443 gate reads: *"Only
  meaningful once there's an old generation to duplicate alongside — a first build
  has nothing to keep serving draws, so it always goes straight through the chunked
  path below."* The code does the opposite: the chunked block is entered only when
  `has_existing_buffers && duplicate_is_safe`, so a first build
  (`has_existing_buffers == false`) skips it entirely and falls through to
  `rebuild_geometry_ssbo_atomic_fallback`. That is the correct behaviour — there is
  nothing to keep serving, so a synchronous build is right — but the sentence
  describing the gate states the wrong branch, in the one comment a future auditor
  reads to confirm #2374's device-loss protection is intact.
- **Evidence**:
  - `mesh.rs:1385`: `let duplicate_is_safe = !geometry_rebuild_needs_idle(projected_bytes, has_existing_buffers);`
  - `mesh.rs:1386`: `if has_existing_buffers && duplicate_is_safe {` … `return self.advance_geometry_rebuild(...)`
  - `mesh.rs:1435`: unconditional fall-through to `rebuild_geometry_ssbo_atomic_fallback`.
  - `geometry_rebuild_needs_idle` (`mesh.rs:228-233`) returns `false` whenever
    `has_existing_buffers` is `false`, which is what makes `duplicate_is_safe`
    `true` for a first build and the sentence look plausible without being true.
- **Impact**: Doc-level only, but on the exact predicate #3443 was filed about. The
  same paragraph is what a reader would use to decide whether a large first-load
  (FO4 boundary, ~800–900 MiB) is chunked or atomic; it is atomic, and the comment
  says otherwise.
- **Suggested Fix**: Reword to "a first build has nothing to keep serving draws, so
  it skips the chunked path and builds synchronously in the fallback below" — or
  move the sentence next to `geometry_rebuild_needs_idle`, which is what the
  `has_existing_buffers` term it describes actually belongs to.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D5-04

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
