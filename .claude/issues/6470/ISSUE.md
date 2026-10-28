# GAME-D1-2026-10-27-01: `item_count` (GetItemCount CTDA 47 and its ObScript getter) is a twelfth hand-spelled held-count sum, and it does not saturate

Labels: low, gameplay, inventory, tech-debt, bug
Source: docs/audits/AUDIT_GAMEPLAY_2026-10-27.md

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-27.md` finding `GAME-D1-2026-10-27-01` (LOW)
**Location (re-mapped to current tree)**: `crates/scripting/src/condition/eval/actor.rs` (`item_count`, ~:223-236)

**Severity**: LOW. Dimension: Inventory (Dim 1). #6363 / 5b43fe347 swept eleven sites and missed this one (added in f57c75616, #6251).

## Description
`inventory.items.iter().filter(|s| items.contains(&s.base_form_id)).map(|s| s.count).sum()` is the form-list form of the pattern `Inventory::count_of` replaced. The plain `u32` `.sum()` panics on overflow in debug builds and wraps in release, while `count_of` saturates (pinned by the `u32::MAX` test in `inventory.rs`).

## Evidence
`let count: u32 = inventory.items.iter().filter(..).map(|stack| stack.count).sum();` in `item_count` (verified at HEAD).

## Impact
Only for contrived counts above `u32::MAX` summed across rows; the main cost is maintainability - the one-accessor rule is broken at its most-used reader.

## Related
#6363, #6251.

## Suggested Fix
Sum `inventory.count_of(form)` over the form-list members with `saturating_add`, or add an `Inventory::count_of_any(&[u32])`.

## Completeness Checks
- [ ] **SIBLING**: grep for any remaining hand-rolled held-count sums
- [ ] **TESTS**: A regression test pins this specific fix

