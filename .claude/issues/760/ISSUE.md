# #760: SF-DIM2-01: BA2 module docstring claims v2=GNRL / v3=DX10; vanilla ships 15 v2 DX10 archives + cosmetic compression-field threading

URL: https://github.com/matiaszanolli/ByroRedux/issues/760
Labels: documentation, low

---

**From**: `docs/audits/AUDIT_STARFIELD_2026-04-27.md` (Dim 2, SF-DIM2-01 + SF-DIM2-04 bundled)
**Severity**: LOW (docstring drift; code is correct)
**Status**: NEW

## Description

Two cosmetic findings on `crates/bsa/src/ba2.rs` bundled into one PR-sized cleanup.

### SF-DIM2-01 — Docstring claims v2 = GNRL / v3 = DX10; reality is mixed

Module docstring at `ba2.rs:11-13` and inline comment at `163-166` imply v2 is GNRL-only and v3 is DX10-only. Cross-product probe across 108 vanilla Starfield archives:

| version × variant | count |
|-------------------|-------|
| v2 GNRL           | 78    |
| v2 DX10           | 15    |
| v3 DX10           | 15    |
| v3 GNRL           | 0     |

v2 DX10 archives include `Constellation - Textures.ba2`, `OldMars - Textures.ba2`, `Starfield - GeneratedTextures.ba2`, all eight `SFBGS*** - Textures.ba2`, `ShatteredSpace - Textures.ba2`, and four CC `*-textures.ba2`. Code path is variant-agnostic (gates on `version`, not `type_tag`), so behavior is correct — but a future "optimization" might wrongly skip the v2 extension on DX10 or the v3 extension on GNRL.

### SF-DIM2-04 — `compression` field threaded through every dispatch site but unused for v1/v2/v7/v8

`compression` is read once at open time (defaulting to `Zlib`) and threaded through `extract_general` / `extract_dx10` / `decompress_chunk`. For v1/v2/v7/v8 archives the value is always `Zlib`. The threading is correct but auditor-confusing — reading `extract_general(..., self.compression)` suggests "this might LZ4 a v1 archive" when it never can.

## Suggested Fix

1. Reword the module docstring at `ba2.rs:11-13`:
   ```
   //! - **Starfield** — v2 (both GNRL and DX10) extends header by 8 bytes;
   //!   v3 (DX10 only in vanilla, no v3 GNRL observed) extends by 12 bytes
   //!   with `compression_method`: 0 = zlib, 3 = LZ4 block.
   ```
   Sibling-fix the version-mapping table at `ba2.rs:17-23`.

2. Leave the `compression` parameter threading as-is (preferred — forces every dispatch site to think about codec). Just add a one-line comment that v1/v2/v7/v8 always pass `Zlib`.

## Completeness Checks

- [ ] **TESTS**: n/a (doc-only).
- [ ] **SIBLING**: #596 covers a different paragraph in the same file (per-archive vs per-chunk granularity); could be merged into a single PR or kept separate.
- [ ] **DROP / LOCK_ORDER / FFI**: n/a.

## Related

- #596 (FO4-DIM2-06: BA2 docstring per-archive vs per-chunk)
