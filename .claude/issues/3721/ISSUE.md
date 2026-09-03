# #3721 — ESM-2026-08-30-D1-01: a nested group's sub_end is never clamped to the parent group's end

**Severity**: LOW · **Location**: `crates/plugin/src/esm/reader.rs::bounded_group_content_end`, and all 13 call sites
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D1-01)

`bounded_group_content_end` returned `self.group_content_end(header)` — i.e.
`pos + (total_size - header_size)` — with no `.min(parent_end)`. The
nesting-*depth* guard (#3237/#3503) was intact; the *extent* guard (how far a
single level can read) was missing. A corrupt/hostile child GRUP declaring a
`total_size` larger than its parent's remaining content could make the child
walker consume records belonging to the parent or the next top-level group,
silently mis-attributing them rather than failing diagnosably.

## Fix implemented

Added a `parent_end: usize` parameter to `bounded_group_content_end`,
clamping the returned end via `.min(parent_end)` per the issue's own suggested
fix. Every walker already loops against its own `end` bound (`while
reader.position() < end && ...`), so `end` was in scope and threaded through
at all 13 call sites unchanged otherwise.

**SIBLING** (issue's own checklist item): confirmed via
`grep -rn "bounded_group_content_end("` that all 13 recursive GRUP walkers
(`grup_walker.rs` ×4, `cell/wrld.rs` ×1, `cell/walkers.rs` ×2, `cell/support.rs`
×6) pass their real, already-in-scope `end` parameter — none constructs a
placeholder or a widened bound.

**TESTS** (issue's own checklist item):
`bounded_group_content_end_clamps_to_parent_end` builds a synthetic
`GroupHeader` declaring 10,000 content bytes against a parent bound of 200
(natural end would be 10,076) and asserts the returned end is clamped to
exactly 200. A second assertion in the same test confirms a well-formed group
that fits entirely inside its parent is unaffected by the clamp — its own
natural end is still returned unchanged, so the fix doesn't shrink legitimate
nested groups.

Full workspace: `cargo test --no-fail-fast` 7052 passing, 0 failing (+1 new
test).
