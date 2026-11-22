# #7100 — ESM-D1-02: ESM compressed-record inflate is a hand copy of `bsa::safety::inflate_bounded_zlib`, no parity guard

Labels: low,esm-plugin,tech-debt,bug
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D1-02)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D1-02**

- **Severity**: LOW (duplication, not a live divergence)
- **Dimension**: 1, `FLAG_COMPRESSED` bound
- **Record / Sub-record**: any record with `FLAG_COMPRESSED`
- **Location**: `crates/plugin/src/esm/reader.rs:1018-1075` against `crates/bsa/src/safety.rs:176-207`; the doc claim at `reader.rs:1015-1017`; `crates/bsa/src/lib.rs:35` (`mod safety;` is private)
- **Status**: NEW. No open or closed issue found; #3812 ported #3720's recovery into `inflate_bounded_zlib` and left the ESM copy standing.
- **Description**: Both sides run the same algorithm. Each takes `take(declared + 1)` through a zlib decoder. On a zlib error each retries as raw DEFLATE over `[2..]` and accepts only an exact length match. Each ends with a `len <= declared` check. I compared them line by line and they agree, so the checklist's "implementations diverging" regression is **not** present today. But the logic exists twice. The `reader.rs` comment says it mirrors `byroredux_bsa::safety::inflate_bounded` (#3410), which is the generic variant; the zlib sibling `inflate_bounded_zlib` is the actual twin. `safety` is not exported (`pub use safety::{MAX_CHUNK_BYTES, MAX_ENTRY_COUNT}` only), so that path cannot even be named from the plugin crate. `byroredux-bsa` is a normal dependency of the plugin crate (added for `bscrc32`, #6048), so sharing is cheap.
- **Impact**: None today. The next change to the Adler-32 recovery or to the bomb bound, such as the flate2 1.1.10 truncated-stream semantics of #5660, has to be made in two crates by hand. A divergence would be silent.
- **Suggested fix**: `pub use safety::inflate_bounded_zlib` from `byroredux-bsa`. Call it from `read_sub_records` with `declared = decompressed_size`, mapping the error to the form-id context. Keep `record_inflation_ceiling` in the plugin crate. Fix the doc reference.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
