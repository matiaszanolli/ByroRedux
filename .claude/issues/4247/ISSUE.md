# FO3-D5-2026-09-11-01: collision module docstring cites a non-existent example path (_tmp_fo3_d5_collision.rs)

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4247
**Labels**: documentation, nif-parser, low, legacy-compat, game:fo3, doc-rot
**Source**: `/audit-fo3` — `docs/audits/AUDIT_FO3_2026-09-11.md`, finding FO3-D5-2026-09-11-01

**Severity**: LOW
**Dimension**: FO3 Collision Import (Havok → CollisionShape) — `/audit-fo3` Dimension 5
**Location**: `crates/nif/src/import/collision/mod.rs:56-60`

**Description**: The collision module docstring for `examine_collision_kind` cites a non-existent example path (`examples/_tmp_fo3_d5_collision.rs`), which was never committed and doesn't resolve.

**Evidence**: `crates/nif/src/import/collision/mod.rs:56-60` states `examine_collision_kind` is "a diagnostic entry point for corpus scans (`examples/_tmp_fo3_d5_collision.rs`)". No file of that name exists anywhere in the repository; `crates/nif/examples/` exists and holds many other real, committed probe examples, but not this one.

**Impact**: Self-inflicted doc rot invisible to the path-validate gate (it only scans `.claude/commands/*.md`, not `.rs` docstrings). Points the next FO3 dimension-5 auditor at a file that doesn't exist.

**Suggested Fix**: Italicize as a deleted throwaway, or commit the probe under `crates/nif/examples/` and backtick the real path.

## Completeness Checks
- [ ] **TESTS**: N/A — documentation-only fix; no regression test applicable.
