# #3616: OBL-D3-04: only the last response of a multi-response INFO survives — NAM1/TRDT assign instead of push, losing 4,617 segments

**Source**: `docs/audits/AUDIT_OBLIVION_2026-08-30.md` — Dimension 3 (ESM Record Coverage)
**Severity**: MEDIUM
**Location**: `crates/plugin/src/esm/records/misc/dialogue.rs` — the `b"NAM1"` and `b"TRDT"` arms of `parse_info`

## Description

An INFO record may carry several NAM1/TRDT response segments. Both arms **assign** rather
than push, so only the last response of a multi-response INFO survives.

## Evidence

Verified 2026-08-30:

```rust
b"NAM1" => out.response_text = read_lstring_or_zstring(&sub.data),
```

and the `b"TRDT"` arm assigns the same way. `ResponseText` is a single field, not a `Vec`.

Measured over Oblivion's INFO records: **23,877 NAM1 and 23,877 TRDT occurrences across
19,260 records** — so 4,617 response segments (19.3% more than one per record) are
overwritten and lost.

## Impact

Every multi-line NPC line in Oblivion plays only its final segment. The loss is silent — no
parse error, no warning — and it is not confined to flavour text: multi-segment responses
are the normal authoring shape for longer quest dialogue.

## Suggested Fix

Make the response a `Vec` of (TRDT, NAM1) segments and push on each occurrence, preserving
authored order; consumers that want a single string join them.

## Related

OBL-D3-02, OBL-D3-03 — same parser, same generation gap.

## Completeness Checks
- [ ] **SIBLING**: check the other assign-not-push arms in the same parser for the same repeated-sub-record shape
- [ ] **TESTS**: a regression test pins a real multi-segment Oblivion INFO retaining every NAM1/TRDT pair in order
