# Issue #3393 — SF-2026-08-27-D1-02: both new source-order pin tests slice source text at a fixed byte offset — latent 'not a char boundary' panic

Filed: 2026-08-27 by `/audit-publish` from `docs/audits/AUDIT_STARFIELD_2026-08-27.md`

Labels: `low,bug,test-gap,import-pipeline,game:starfield,legacy-compat`

> Immutable snapshot of the issue as filed (TD10-001 / #1156).
> GitHub is authoritative for current state: `gh issue view 3393 --json state`.

---

Found by `/audit-starfield` — [`docs/audits/AUDIT_STARFIELD_2026-08-27.md`](docs/audits/AUDIT_STARFIELD_2026-08-27.md), Dimension 1, delta review of `1b521305` + `cceee44d`.

- **Severity**: LOW
- **Location**: `crates/bsa/src/ba2.rs:1686` and `crates/bsa/src/ba2.rs:1718`
- **Status**: NEW

## Description

Both recent fixes introduced the same new technique — `include_str!("ba2.rs")`, split on a match-arm marker, then:

```rust
let body = &arm[..arm.len().min(2000)];
```

Rust `str` indexing is by **byte**, and both arms are far longer than 2,000 bytes, so `min(2000)` always resolves to a fixed byte cut into text that contains multi-byte UTF-8. If byte 2,000 ever lands on a continuation byte, the test panics with `byte index 2000 is not a char boundary` — which says nothing about the invariant under test.

## Evidence

Measured against the current file:

- the `Ba2Compression::Lz4Block =>` arm is **41,574 bytes** and contains **3** em dashes (`—`, 3 bytes each) within its first 2,000 bytes;
- the `BA2_V_STARFIELD_V3 =>` arm is **66,773 bytes** with **2**.

Byte 2,000 currently lands on ASCII in both (`…d::InvalidData,\n form` and `… other\n`), so the tests pass today. That is luck about where the comment text happens to end, not a property anyone asserted. Both commits' new comment blocks sit inside the first 2,000 bytes of their respective arm, so any edit to them shifts the cut.

## Impact

A cosmetic comment edit inside either arm can turn a green suite red with an opaque panic — in a test whose entire purpose is to make a *deliberate* regression legible. Blast radius is CI/test only; no runtime path.

## Suggested Fix

Replace `&arm[..arm.len().min(2000)]` with a boundary-safe form:

```rust
let body = arm.get(..2000).unwrap_or(arm);
```

or scope by the next `}` / a line count instead of a byte budget.

## Related

`SF-2026-08-27-D1-01` (#3392) — same two commits. The `_audit-common.md` note that source-order pins are the accepted workaround for the absent log-capture harness still holds: the *technique* is fine, the *slicing* is not.

**Same defect class as the HIGH #3391** (`canonical_mesh_path` byte-slicing a `&str` at a computed offset). Two unrelated commits landed the same week, found independently by two audit dimensions. A `clippy::string_slice` lint would catch both and is probably the right fix at the workspace level rather than two point repairs.

## Completeness Checks
- [ ] **SIBLING**: sweep for other `&s[..n]` / `&s[n..]` on `&str` with a computed `n` across the workspace (see #3391)
- [ ] **TESTS**: the pin tests still fail on the regression they exist to catch after the change
