# #1767: SCR-D6-NEW-01: condition::evaluate panics (index OOB) when a CTDA list ends with or_next == true

Filed from `docs/audits/AUDIT_SCRIPTING_2026-06-27.md` on 2026-06-27. Snapshot as-filed (GitHub is authoritative for live state).

**Severity**: HIGH · **Dimension**: Scripting Runtime — condition evaluator / CTDA OR-precedence · **Untrusted-Input**: Yes
**Location**: `crates/scripting/src/condition.rs:405-417` (`evaluate`; panic at the range index)
**Status**: NEW (pre-existing miss — the OR-block loop dates to 2026-06-09, before the first audit's ✅ on OR-precedence)
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-06-27.md` (SCR-D6-NEW-01)

## Description
The OR-block discovery loop walks `i` while `conditions[i].or_next`, then sets `block_end_inclusive = i` and evaluates `(block_start..=block_end_inclusive)`:

```rust
while i < conditions.len() && conditions[i].or_next { i += 1; }
let block_end_inclusive = i;            // == conditions.len() when the LAST or_next is set
...
(block_start..=block_end_inclusive).any(|j| evaluate_condition(&conditions[j], world, ctx))
```

When the **final** condition has `or_next == true`, the loop walks `i` to `conditions.len()`, so `block_end_inclusive == len` and the inclusive range indexes `conditions[len]` → out of bounds. The `.any()` short-circuit masks it only when an earlier OR-block member evaluates `true`; if every preceding member is `false`, evaluation reaches the OOB index and **panics**.

## Evidence
Injected test (reverted): `evaluate(&vec![cond(99999, Ne, 0.0, /*or_next=*/true)], …)` → `panicked at crates/scripting/src/condition.rs: index out of bounds: the len is 1 but the index is 1`. The plugin parser sets `or_next` straight from `type_byte & 0x01` with no clamp; nothing clears a trailing OR flag. `evaluate` is live on `quest_advance_system` and is the shared entry point for every future CTDA consumer (perks, dialogue INFOs, AI packages, magic effects). The prior audit's matrix marked CTDA OR-precedence ✅ — that verified the grouping *semantics*, not this trailing-`or_next` boundary.

## Impact
A malformed / hand-edited / truncated ESP whose condition tail leaves the OR bit set crashes the engine the first frame the predicate is evaluated — one bad CTDA byte takes down cell load / activation / quest advance. Silent in `cargo test` (no test exercises a trailing-OR list).

## Suggested Fix
Clamp after the inner loop: `let block_end_inclusive = i.min(conditions.len() - 1);` (the `while i < len` guarantees `len ≥ 1`). A trailing OR flag then harmlessly terminates the final block at its last real member, matching the "last condition's or_next is meaningless" contract the doc-comment already asserts. Add a trailing-`or_next` regression whose members all evaluate false.

## Completeness Checks
- [ ] **SIBLING**: check `evaluate_condition` and any other `..=`/`for j in start..=end` index over `conditions` for the same off-by-one on a trailing flag
- [ ] **TESTS**: regression with a trailing-`or_next` condition list whose OR members all evaluate false (asserts `false`, not panic)
