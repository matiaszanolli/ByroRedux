# RT-2026-08-16-02: p0 and p1 smoke gates are deterministically RED — one reworded log line broke both

**Issue**: #3001
**Severity**: HIGH
**Dimension**: Playable-slice gate semantics
**Labels**: `high,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_RUNTIME_2026-08-16.md` (Dimension — Playable-slice gate semantics).

**Location**: `docs/smoke-tests/p0-door-interaction.sh`:115 · `docs/smoke-tests/p1-character-traversal.sh`:251 · `byroredux/src/interaction.rs`:492

## Description

`input.press`'s success message was reworded. Both the P0 and P1 gates grep for the **old** wording, so both are deterministically RED against the current engine — and have been since the rewording landed three commits ago.

## Evidence

Current emitter (`byroredux/src/interaction.rs`:491-494):
```rust
Ok(format!(
    "input.press: queued {} through the {label} binding",
    action.label()
))
```
with `action.label()` → `"Activate"` and `binding_label(action)` → `key_label(key)` → `"E"`.

So the live line is: **`input.press: queued Activate through the E binding`**

What the gates expect:
```bash
# p0-door-interaction.sh:115
"input.press: queued KeyE through the normal Activate binding"
# p1-character-traversal.sh:251
grep -Fq "normal Activate binding" "$command_log"
```

Neither matches — the first slot changed from the key name to the action label, and the word `normal` no longer appears anywhere in the message.

**The tell**: `p2-melee-core.sh`:148 greps `"queued Attack through the R binding"` — the *new* format. P2 was updated with the rewording; P0 and P1 were not.

Re-verified 2026-08-17 against the live source.

## Impact

Two of the three playable-slice gates cannot pass. Because RT-2026-08-16-04 (#3003) means no CI job runs them and they `exit 0` when game data is absent, nothing surfaced the breakage — the gates are simultaneously broken and silent.

## Suggested Fix

Update both greps to the current message. Better: have the scripts assert on a stable token (the action label alone) rather than the full sentence, so a future rewording of the surrounding prose does not break them again — or pin the message with a unit test so the engine side cannot drift unnoticed.

## Related

- #3003 (RT-2026-08-16-04 — no CI job runs these gates, which is why this went unnoticed)
- #2731 (the `main.rs` split era in which the rewording landed)

## Completeness Checks
- [ ] **SIBLING**: Every grep in all three smoke scripts re-validated against live engine output, not just the two named lines
- [ ] **STABLE-TOKEN**: The assertions key on something less brittle than a full prose sentence
- [ ] **NO-SILENT-RED**: Paired with #3003 so a broken gate fails loudly instead of skipping
- [ ] **TESTS**: The `input.press` message is pinned engine-side so the scripts cannot silently desync again

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3001 --json state` when live state is needed.*
