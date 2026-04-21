# Issue #500: debug_assert! tuple order mismatches actual sort key

Severity: MEDIUM (debug-only correctness)
Location: draw.rs:470-479 vs render.rs:597-619

## Problem
debug_assert had (alpha_blend, two_sided, is_decal); real sort key has
(alpha_blend, is_decal, two_sided).

## Fix
Option (a) — removed assert, extracted `draw_sort_key` helper, added
unit tests in render.rs so the contract lives in one crate.
