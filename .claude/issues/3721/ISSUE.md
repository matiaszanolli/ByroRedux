# #3721 — ESM-2026-08-30-D1-01: a nested group's sub_end is never clamped to the parent group's end

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: LOW · **Dimension**: Header & GRUP Walk
**Location**: `crates/plugin/src/esm/reader.rs` (`bounded_group_content_end`, ~:859-877), and every call site
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D1-01)

## Description

`bounded_group_content_end` returns `self.group_content_end(header)` — i.e. `pos + (total_size - header_size)` — with no `.min(parent_end)` and no `.min(data.len())`. The depth guard it adds (#3237/#3503) is intact; the *extent* guard is missing.

A corrupt or hostile child GRUP declaring a `total_size` larger than its parent's remaining content makes the child walker consume records belonging to the parent (or to the next top-level group); the parent loop then exits early because `position() > end`.

## Impact

Not a memory-safety issue — every loop also tests `reader.remaining() > 0` and `read_record_header` returns `Err` on truncation — but the result is silent mis-attribution rather than a diagnosable failure. **No vanilla master triggers it.**

## Suggested Fix

`Some(self.group_content_end(header).min(parent_end))`, threading the parent end (already in scope as `end` at every call site).

## Completeness Checks
- [ ] **SIBLING**: All 13 recursive GRUP walkers pass a real parent end, not a placeholder
- [ ] **TESTS**: A synthetic-fixture test builds a child GRUP overrunning its parent and asserts the overrun is clamped
