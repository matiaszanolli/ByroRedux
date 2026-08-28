# #3527 — SF-2026-08-27b-D1-01: the #3393 fix orphaned the #2097 panic-guard rationale onto a string helper

Source: `docs/audits/AUDIT_STARFIELD_2026-08-27b.md`
Filed: 2026-08-28 (`/audit-publish`)
Labels: low, documentation, doc-rot, import-pipeline, game:starfield, legacy-compat

---

From `docs/audits/AUDIT_STARFIELD_2026-08-27b.md` (branch `main` @ `969d81c8`).

- **Severity**: LOW
- **Dimension**: 1 — BA2 LZ4 (delta review of `caa14cc5`)
- **Location**: `crates/bsa/src/ba2.rs` — the `#2097 / LZ4-01` doc comment, now heading `prefix_up_to`; and `lz4_decompress_is_panic_guarded`, now undocumented

## Description

`caa14cc5` inserted the new `prefix_up_to` helper **between** `lz4_decompress_is_panic_guarded`'s doc comment and the test it documents. The two comment blocks fused, so the file now reads:

```rust
/// #2097 / LZ4-01 — pins that the LZ4 arm still routes through
/// `catch_unwind`.
///
/// … delete the `catch_unwind` and this test fails, which is the
/// only way this fix can be kept from silently regressing on a future
/// dependency bump — exactly the scenario the issue was filed about.
/// #3393 — take at most `max_bytes` of `s`, backing up to the nearest
/// char boundary.
…
fn prefix_up_to(s: &str, max_bytes: usize) -> &str {
```

`prefix_up_to` is now documented as pinning a `catch_unwind` it has nothing to do with, and `lz4_decompress_is_panic_guarded` has **no doc comment at all**.

## Evidence

Read directly from the current file (verified again at publish time: the fused block heads `prefix_up_to`, and the `#[test] fn lz4_decompress_is_panic_guarded` immediately below `prefix_up_to_backs_up_to_a_char_boundary` carries no doc). The fusion is visible in `git diff bbfd742f..HEAD -- crates/bsa/src/ba2.rs`, where the `#3393` block is added immediately after the existing `#2097` block with no separating item. The `#3392` and `#3393` test doc comments below it are correctly attached, so this is a single misplacement, not a systematic one.

**Attempts to disprove**: not a rendering artefact of the diff — `rustdoc` and the raw file agree that both blocks precede `prefix_up_to`; not harmless by position — `prefix_up_to` is `#[cfg(test)]`-module-local and genuinely deletable, unlike the test.

## Impact

Cosmetic today. The cost lands later: `#2097`'s rationale — an explicit "this test exists so a future dependency bump cannot silently remove the guard" — is the sort of comment that gets deleted alongside a helper someone decides is unnecessary. The guard would survive; its reason would not.

## Related

#3393 (CLOSED, `caa14cc5`), #2097, #3392. Same defect *shape* as #3493 (`apply_effect`) and #3494 (`water.rs`) filed by concurrent passes — different files, no overlap.

## Suggested Fix

Move the `#2097 / LZ4-01` block back down to immediately precede `#[test] fn lz4_decompress_is_panic_guarded`, leaving `prefix_up_to` with only its own `#3393` doc. Three-line move, no behaviour change.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — the other doc blocks in the same test module, and the two concurrently-filed instances of this shape (#3493, #3494)
- [ ] **TESTS**: A regression test pins this specific fix
