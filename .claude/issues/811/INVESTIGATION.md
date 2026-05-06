# Investigation — #811

**Domain**: bsa (BA2 reader)

## Code path

`Ba2Archive::open` (`crates/bsa/src/ba2.rs:115-`) reads a 24-byte base header, then dispatches header-extension reads on `version == 2 || version == 3` and `version == 3` via two cascading `if`s (lines 173-197). Versions outside that pair (0, 4, 5, 6, 9, 10, ..., u32::MAX) skip both arms and fall through to `read_general_records` / `read_dx10_records` as if they were the 24-byte v1 layout. No allowlist check anywhere.

The module docstring at lines 20-26 explicitly commits to the allowlist `{1, 2, 3, 7, 8}` (BTDX v1 = FO4 original / FO76, v2/v3 = Starfield, v7/v8 = FO4 Next Gen). Implementation is out of sync with documentation.

## Sibling reader

`BsaArchive::open` (`crates/bsa/src/archive.rs:165-173`) already has the right allowlist pattern — explicit `version != 103 && version != 104 && version != 105` check returning `InvalidData` with a "(expected 103, 104, or 105)" message. No sibling fix needed; just bring BA2 in line with the BSA pattern.

## Approach

Replace the two cascading `if`s with a single `match version` over `{1, 2, 3, 7, 8}` returning the per-version compression. Default arm returns `InvalidData` listing the supported set. The compression dispatch nests inside the `3` arm so the unknown-method check (already covered by `v3_unknown_compression_method_rejected`) stays scoped.

Add `unknown_version_rejected` test mirroring the existing two regression tests' shape (build a 24-byte header in a temp file, expect `InvalidData` from `Ba2Archive::open`).

## Scope

1 file: `crates/bsa/src/ba2.rs`. No public API change — `Ba2Archive::open` already returns `io::Result`; the new error path just adds an enumerated rejection reason.
