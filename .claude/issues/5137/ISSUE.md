# #5137: SPT-2026-09-29-D3-02: the streaming prefetch's `.spt` skip byte-slices a `&str`, so a non-ASCII MODL tail panics the cell's pre-parse

**Labels**: low, bug, speedtree, terrain-exterior

**Source report**: `docs/audits/AUDIT_SPEEDTREE_2026-09-29.md` (report ID `SPT-D3-02`)
**Severity**: LOW
**Dimension**: TREE→Billboard Wiring

## Location
`byroredux/src/streaming.rs` (`PreflightDecision::SkipSpt` arm); input decode at `crates/plugin/src/esm/records/common.rs` (`read_mesh_path` → `read_zstring`)

## Description
`model_path[model_path.len() - 4..].eq_ignore_ascii_case(".spt")` slices by byte index. Introduced by `a2aae52a7` (#4207, 2026-09-21).
- MODL strings come from `read_mesh_path` → `read_zstring` → `String::from_utf8_lossy`, which rejects ASCII control bytes but accepts non-ASCII.
- A cp1252 byte decodes to U+FFFD (3 bytes), and mod content can author UTF-8 paths. A model path whose 4th-from-last byte lies inside a multi-byte character (e.g. `x\u{FFFD}ab`) panics with "byte index is not a char boundary".
- Every other `.spt` test in the tree is char-safe: `to_ascii_lowercase().ends_with(".spt")` in `synth_child.rs` and `nif_loader.rs`, and `rsplit('.')` in `nif_loader.rs`.

## Evidence
`let d = if model_path.len() >= 4 && model_path[model_path.len() - 4..].eq_ignore_ascii_case(".spt")`. The `catch_unwind` in `streaming.rs` logs "panic in pre_parse_cell … recovered with empty payload (#854)".

## Impact
A malformed or non-ASCII mod MODL on any REFR throws away the whole cell's worker pre-parse; the cell then loads on the main-thread sync path (a hitch) and a panic line is logged. No crash, no data loss. Vanilla paths all end in an ASCII extension and are unaffected.

## Related
#3391 (closed; same char-boundary class in `canonical_mesh_path`), #4207, #3735.

## Suggested Fix
Test the bytes instead: `model_path.as_bytes().len() >= 4 && model_path.as_bytes()[model_path.len() - 4..].eq_ignore_ascii_case(b".spt")`. Add a unit case with a trailing U+FFFD.

Validated at HEAD 9fcfdc3fc: the byte-index slice is still present in `streaming.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other `path[len-N..]` extension checks)
- [ ] **TESTS**: A regression test pins this specific fix
