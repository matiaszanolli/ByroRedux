# NIF-D3-NEW-04: NiSourceTexture::Use Internal cond gate has no regression test

**Severity**: LOW (defensive — current code is correct)
**Source audit**: `docs/audits/AUDIT_NIF_2026-05-10.md` (Dim 3)

## Location

`crates/nif/src/blocks/texture.rs:57`

## Why it's a bug

The cond on `Use Internal` is `Use External == 0` (per nif.xml line 5117). ByroRedux gates with `if !use_external && stream.version() < V10_0_1_3 { ... }`. The cond order is currently correct.

If a future contributor reorders or flips the boolean (a common refactor mistake), the parser will read the byte when `use_external == true`, drifting by 1 byte on every external texture (which is most of them).

No test pins the cond ordering.

## Fix

Regression test: build a v10.0.1.2 `NiSourceTexture` with `use_external == true` and verify `parse()` consumes exactly the expected bytes.

## Completeness Checks

- [ ] **TESTS**: Add the cond-gate regression test in `texture.rs::tests` or `parse_real_nifs.rs`
