# REG-2026-08-20-D2-02: #3049's max_log_message_bytes ceiling test satisfied by the sibling ceiling

**Issue**: #3215 — https://github.com/matiaszanolli/ByroRedux/issues/3215
**Severity**: LOW
**Labels**: `low,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_REGRESSION_2026-08-20.md`
**Filed**: 2026-08-20 · `/audit-publish` · verified against HEAD `bb0b92f2`

---

Filed from `docs/audits/AUDIT_REGRESSION_2026-08-20.md` § REG-2026-08-20-D2-02 (Dimension 2 — Guard existence & liveness).

**Severity**: LOW
**Location**: `crates/mod-runtime/src/limits.rs:167-174` (the shared `||` chain), `:330-341` (`oversized_max_log_message_bytes_is_rejected`).

## Description

**#3049** added ceilings to `SandboxConfig::validate()` and, to its credit, a table-driven completeness test plus two extra tests for the log fields the table omits. **One of those two cannot fail for the field it names.**

All three log ceilings share a **single** `||` arm returning **one** error:

```rust
// limits.rs:167 — one arm, three fields, one error
if self.max_log_entries > MAX_SANE_LIMIT
    || self.max_log_message_bytes > MAX_SANE_LIMIT
    || self.max_log_bytes > MAX_SANE_LIMIT
{ return Err(SandboxError::InvalidConfig("a log limit exceeds the sane ceiling")); }
```

The test sets `max_log_message_bytes = MAX_SANE_LIMIT + 1` **and** `max_log_bytes = MAX_SANE_LIMIT + 2`, so the `max_log_bytes` clause alone rejects the config. The assertion is `matches!(…, Err(SandboxError::InvalidConfig(_)))` — a **wildcard that cannot distinguish which clause fired**.

**Delete the `self.max_log_message_bytes > MAX_SANE_LIMIT` term and the test stays green.**

## Evidence (verified at HEAD `bb0b92f2`)

The test's own docstring explains why the second field was raised:

> *"with `max_log_bytes` raised alongside it so the pre-existing cross-check doesn't fire for an unrelated reason and mask which guard actually caught it."*

— which fixes one masking and **introduces another**. The `max_log_message_bytes > max_log_bytes` cross-check at `:175` is indeed sidestepped (`MAX+1 < MAX+2`), but only by pushing the sibling over the very ceiling under test.

The sibling `oversized_max_log_bytes_is_rejected` (`:315-324`) shows the correct construction **one function above it** — it raises a single field.

## Impact

Low. `crates/mod-runtime` has no engine consumer yet and these are explicitly a *sanity backstop*, not derived physical limits.

Filed because it is a clean, verified instance of this sweep's theme — *can the guard actually fail?* — in code closed two days ago, and because the correct construction is literally adjacent.

The other nine ceilings **are** properly guarded, and `oversized_wasm_stack_is_rejected` / `wasm_stack_at_the_ceiling_is_accepted` correctly bracket the `>` vs `>=` boundary.

## Suggested Fix

Preferred: **split the `||` chain into three arms with distinct messages** and match the message. That makes all three fields independently falsifiable and is the smaller change.

Alternative: drop `max_log_bytes` from the fixture (leave it at its 1 MiB default — `MAX_SANE_LIMIT + 1` for the message already exceeds it) and assert on the *cross-check* instead.

## Related

- **#3049** (`SAFE-2026-08-16-02`) — the fix this guards; `9725baeb`
- **#2543** (`MAX_SANE_SHAPE_EXTENT`) — the posture #3049 cites as precedent
- The `#3089` guard-misses-the-call-site finding filed from this same report — the MEDIUM sibling of this shape

## Completeness Checks
- [ ] **FALSIFIABLE**: Deleting the `max_log_message_bytes > MAX_SANE_LIMIT` term makes the test **fail** — verify by deleting it locally
- [ ] **SIBLING**: The `max_log_entries` clause is independently falsifiable too, not just the two named fields
- [ ] **TESTS**: If the `||` chain is split, the table-driven completeness test is updated to cover the new distinct messages

## Label note

`crates/mod-runtime` has no matching domain label in this repo — filed with severity + `tech-debt` + `bug` only.
