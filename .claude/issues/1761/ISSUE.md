# #1761 — TD8-004: Dx10Chunk::start_mip now read — its #[allow(dead_code)] is redundant; end_mip set-never-read

**Severity**: LOW · **Dimension**: 8 — Dead Code
**Location**: `crates/bsa/src/ba2.rs::Dx10Chunk`

## Fix

Verified the premise against current code: `start_mip` is read at 4 sites
(the monotonic-order validation `chunks.windows(2).all(|w| w[0].start_mip
<= w[1].start_mip)` plus its two warning-message `.map(|c| c.start_mip)`
calls), so its `#[allow(dead_code)]` is redundant. `end_mip` is written at
construction and never read anywhere — confirmed via a fresh grep, matching
the issue's own evidence exactly.

Applied the issue's own suggested lowest-risk fix: removed `start_mip`'s
now-redundant `#[allow(dead_code)]`, kept `end_mip`'s as the documented
M40 (#1049) streaming reserve, and extended the existing M40 doc comment
to record why the two fields are no longer treated identically.

## TESTS (issue's own checklist item)

The issue's own stated test is structural, not behavioral: `cargo build
-p byroredux-bsa` staying clean with `start_mip`'s attribute removed is
itself the proof that it's a live read (the compiler's own dead-code
lint is the check here — no dedicated `#[test]` fn adds anything a
compiler warning doesn't already cover for an attribute-only change with
no runtime behavior).

**Verification, not reintroduce-and-revert** (no behavioral test to
revert): confirmed `cargo check -p byroredux-bsa --tests` is clean with
zero warnings after removing the attribute — if `start_mip` were not
actually read, `#[warn(dead_code)]` (on by default) would have fired.

## Verification

- `cargo check -p byroredux-bsa --tests`: clean, zero warnings (proves
  `start_mip` is genuinely read).
- `cargo test -q -p byroredux-bsa`: all non-ignored tests passing (the
  ignored ones need real BSA/BA2 game data on disk, unaffected either
  way).
- `cargo test -q --no-fail-fast` (full workspace): **7159 passing, 0
  failing** (no new tests — attribute-only change).
