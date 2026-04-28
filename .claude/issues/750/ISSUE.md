# #750: SF-D3-02: `bgsm` crate doc-comment falsely advertises Starfield support

URL: https://github.com/matiaszanolli/ByroRedux/issues/750
Labels: documentation, nif-parser, high, legacy-compat

---

**From**: `docs/audits/AUDIT_STARFIELD_2026-04-27.md` (Dim 3, SF-D3-02)
**Severity**: HIGH (correctness contract / documentation drift)
**Status**: NEW

## Description

`crates/bgsm/src/lib.rs:1` reads:

> `//! Fallout 4 / Skyrim SE / FO76 / Starfield external material file`

The crate parses BGSM v1-v22 + BGEM v1-v22. **Starfield does not ship `.bgsm` or `.bgem` files** — it uses `.mat` JSON descriptors plus a global `materialsbeta.cdb` component database, neither of which this crate handles.

The `version >= 22` branch in `bgem.rs:124` is the highest version the parser understands; anything beyond v22 silently stops reading and returns a file with the trailing fields at defaults — but Starfield doesn't even reach this code path because there is no .mat parser at all.

## Evidence

```rust
// crates/bgsm/src/lib.rs:1
//! Fallout 4 / Skyrim SE / FO76 / Starfield external material file

// crates/bgsm/src/bgsm.rs:12
//! Fallout 4 / Skyrim SE / FO76 lit material file.

// crates/bgsm/src/bgem.rs:124 — caps at v22
```

Inconsistent within the same crate. `bgsm.rs:12` correctly omits Starfield; `lib.rs:1` claims it.

## Impact

Documentation footgun: a contributor reading `lib.rs` first will assume Starfield material reading is shipped. It isn't.

## Suggested Fix

Update `lib.rs:1`:

```rust
//! Fallout 4 / Skyrim SE / FO76 external material file (BGSM v1-v22 / BGEM v1-v22).
//!
//! Not supported: Starfield uses `.mat` JSON + `materialsbeta.cdb` (a different
//! format). See <tracking issue> for the Starfield material parser.
```

Cross-link a new tracking issue for the Starfield `.mat` / `.cdb` parser (filed separately as SF-D6-03 / SF-D3-03 follow-up).

## Completeness Checks

- [ ] **TESTS**: n/a (doc-only).
- [ ] **SIBLING**: Verify ROADMAP.md doesn't claim BGSM-based Starfield material support.
- [ ] **DROP / LOCK_ORDER / FFI**: n/a.
