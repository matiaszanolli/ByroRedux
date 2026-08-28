# #3531 — SPT-2026-08-28-D1-01: is_plausible_spt_curve_string accepts a zero-length candidate, leaving a 4-byte residue of #1822

**Labels**: low, speedtree, terrain-exterior, bug
**Filed from**: `docs/audits/AUDIT_SPEEDTREE_2026-08-28.md` (`/audit-publish`, 2026-08-28)

---

**Severity**: LOW
**Dimension**: Walker Byte-Accounting
**Source**: `docs/audits/AUDIT_SPEEDTREE_2026-08-28.md` — SPT-2026-08-28-D1-01
**Status as reported**: NEW (residual of #1822, *not* a regression of it — the #1822 fix is
verified in place and non-regressive against byte-identical corpus bail offsets)

**Location**: `crates/spt/src/parser.rs:171-176`, reached from the `MaybeStringElseBare` arm at
`:120-136`

## Description

#1822's fix gates the tag-13005 `String` arm on the peeked candidate bytes being
printable-ASCII curve text:

```rust
// crates/spt/src/parser.rs:171-176
fn is_plausible_spt_curve_string(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .all(|&b| matches!(b, 0x20..=0x7E | b'\t' | b'\n' | b'\r'))
}
```

`Iterator::all` is vacuously `true` on an empty slice, and `SptStream::peek_string_lp_bytes`
(`stream.rs:119-135`) returns `Some(&[])` for a declared length of `0`. So a bare `13005`
sitting immediately before a geometry tail whose leading `u32` is `0` still takes the `String`
arm, consumes 4 bytes as an empty string, and shifts `tail_offset` 4 bytes past the true tail
start — the exact failure mode #1822 was filed to close, for the one candidate length the
printable-ASCII discriminator cannot discriminate. A leading `0` (a zero count or index) is a
perfectly ordinary thing for a binary tail to begin with.

## Evidence

- `parser.rs:171-176` and `stream.rs:132-134` (`self.bytes.get(start..end)` with `end == start`
  yields `Some(&[])`).
- The #1822 regression guard `tag_13005_before_geometry_tail_resolves_as_bare_not_swallowed_string`
  (`parser.rs:437-463`) deliberately uses a leading tail value of `8`, not `0`, so the
  empty-candidate arm is untested in either direction.
- Not observed in the corpus: the 4 real bimodal-13005 files (`treems14canvasfreesu`,
  `shrubms14boxwood`, `treecottonwoodsu`, `treems14willowoakyoungsu`) all carry the 104-byte
  `BezierSpline` blob, and this cycle's corpus run reproduces their bail offsets exactly.

## Impact

Bounded and today theoretical — 4 bytes of `tail_offset` drift plus one spurious empty
`SptValue::String` entry, on a file shape not present in the 133-file vanilla corpus. The
geometry tail is not decoded in Phase 1, so nothing consumes the drifted offset yet; it would
matter to the Phase 2 tail decoder. Filed so the hole is closed *before* a consumer exists
rather than after.

## Related

- #1822 — the fix this is the residue of.
- #999 — the original bimodal-13005 handling.

## Suggested Fix

**Which arm is correct for a zero-length candidate is a format question, not a code-style one,
and the audit does not answer it** — `Bare` consumes 0 bytes and `String` consumes 4, so
guessing wrong just moves the desync. Settle it by dumping the four known bimodal files (and any
mod-content sample) with `cargo run -p byroredux-spt --features recon --example spt_dissect` to
see whether a zero-length 13005 payload occurs at all, record the answer in
`crates/spt/docs/format-notes.md`'s "tag 13005 bimodal payload" section, and only then add the
one-line guard (`!bytes.is_empty() &&`) or an explicit zero-length arm. Add a
`tag_13005_before_zero_leading_tail` test either way, since the behaviour is currently unpinned.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — any other vacuous-`all` predicate over a peeked slice in `crates/spt/src/parser.rs` / `stream.rs`
- [ ] **TESTS**: A regression test pins this specific fix (`tag_13005_before_zero_leading_tail`, whichever arm the format answer selects)
