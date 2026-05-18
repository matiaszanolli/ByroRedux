# TD4-301: stale MAX_BONES_PER_MESH math in skin_compute.rs:263 (was 128, now 144)

## Source Audit
`docs/audits/AUDIT_TECH_DEBT_2026-05-17.md` — Dimension 4 (Magic Numbers / doc drift)

## Severity
**LOW** — doc-comment drift after today's #1135 bumped `MAX_BONES_PER_MESH` 128 → 144.

## Location
`crates/renderer/src/vulkan/skin_compute.rs:263`

## Description
The doc comment for the `max_slots` rationale reads:
> The architectural ceiling is `MAX_TOTAL_BONES / MAX_BONES_PER_MESH = 32768 / 128 = 256`

Today's #1135 raised `MAX_BONES_PER_MESH` from 128 to 144 (to cover FO76 vanilla skeletons up to 133 bones). The math is now `32768 / 144 ≈ 227`.

## Proposed Fix
Update the comment to:
- `32768 / 144 ≈ 227` (literal), OR
- reference the constants directly: `MAX_TOTAL_BONES / MAX_BONES_PER_MESH` (let the names carry the value, immune to future bumps)

Prefer the second form.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Sweep for any other doc comments still claiming `128` bones (use `grep -RIn "128.*bones\|/ 128" crates byroredux`)
- [ ] **DROP**: N/A
- [ ] **TESTS**: N/A (comment-only)
