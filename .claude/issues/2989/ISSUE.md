# ESM-2026-08-16-D2-01: parse_with_consumed's doc promises a clean-finish signal its return type cannot carry

**Issue**: #2989
**Severity**: LOW
**Dimension**: 2 — Sub-Record Byte Accounting
**Labels**: `low,import-pipeline,bug`
**Source report**: `docs/audits/AUDIT_ESM_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_ESM_2026-08-16.md` (Dimension 2 — Sub-Record Byte Accounting).

**Record / Sub-record**: `VMAD`
**Location**: `crates/plugin/src/esm/records/script_instance.rs`:172-175, :236, :297-313, :370-372

## Description

`parse_with_consumed`'s doc promises the caller can distinguish a clean finish from a graceful break:

> On a truncated/unknown-type scripts section the offset marks how far the graceful decode got; a fragment decoder should treat a short read as "no fragments" rather than seeking into garbage.

But the function returns only `(Self, usize)` — **there is no success signal**. `parse_quest_fragments` (:298-299) and `parse_scene_fragments` (:371) seek to `consumed` unconditionally, so the sole protection against reading a truncated scripts section's tail as a fragment table is a 1-in-256 `c.u8() == Some(2)` version test.

## Evidence

```rust
// script_instance.rs:175 — no success channel in the return type
pub fn parse_with_consumed(data: &[u8]) -> (Self, usize) {
```

```rust
// script_instance.rs:298-299 — the seek is unconditional
let (_, consumed) = ScriptInstanceData::parse_with_consumed(vmad);
let Some(section) = vmad.get(consumed..) else {
```

The rest of the cursor is **exemplary** — `checked_add` + `slice::get` everywhere, array capacity clamped to 4,096, recursion depth pinned at 1 — which is what makes the one unenforced contract worth naming rather than a symptom of general sloppiness.

## Impact

A truncated or unknown-type VMAD scripts section can have its tail bytes interpreted as a fragment table, gated only by a one-byte version check that passes 1 time in 256 by chance.

Bounded by the cursor's own `slice::get` discipline — this is a mis-parse, not a panic or OOB read. LOW because vanilla data is well-formed; it is the untrusted-mod path that makes it worth closing.

## Suggested Fix

Return a success signal — `(Self, usize, bool)`, or `Option<usize>` for the consumed offset — and have both fragment decoders treat a graceful break as "no fragments" rather than seeking. That is what the doc already promises.

## Related

- #2988 (ESM-2026-08-16-D3-01 — same file, same `parse` family)

## Completeness Checks
- [ ] **SIBLING**: Both `parse_quest_fragments` and `parse_scene_fragments` updated, not just one
- [ ] **DOC-TRUTH**: The docstring's promise matches the signature after the fix
- [ ] **FUZZ-SHAPE**: A truncated VMAD fixture asserts "no fragments" rather than a garbage table
- [ ] **TESTS**: A regression test pins this specific fix

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 2989 --json state` when live state is needed.*
