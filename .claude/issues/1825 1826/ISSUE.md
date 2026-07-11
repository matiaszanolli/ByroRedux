# #1825: FO4-D3-01: BA2 DX10 chunk_hdr_len read but never used to advance the reader

Severity: LOW · Dimension: BA2 Reader
Location: `crates/bsa/src/ba2.rs:538-551, 580-582`

`read_dx10_records` reads `chunk_hdr_len` from `base[14..16]` and
`debug_assert_eq!`s it to 24, but the chunk loop unconditionally reads
a fixed `[0u8; 24]` per chunk — release builds compile out the assert,
so a `chunk_hdr_len != 24` archive would misparse silently with no
telemetry. Zero impact on vanilla FO4 (all confirmed 24).

Suggested fix: add a release-path `log::warn!` when `chunk_hdr_len !=
24` (keep tolerant clamp-to-24 behavior), matching the `num_mips==0`
(:568) and non-monotonic (:628) sibling patterns. Add a regression
test constructing a synthetic DX10 record with chunk_hdr_len != 24.

# #1826: FO4-D4-01: parse_sub_index recovery can desync the stream when block_size is None

Severity: LOW · Dimension: NIF BSVER 130
Location: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:639-654`

When `BsSubIndexTriShapeData::parse` errors AND block_size == None, the
recovery only rewinds to segmentation_start with no compensating skip
— unlike the Some(size) arm which computes and re-skips properly. The
live dispatcher always passes Some(_), so None is currently
unreachable. Suggested fix: if parse_sub_index ever gains a size-less
caller, make the None arm return Err instead of a bare rewind so the
outer loop's block_size resync stays the single source of truth — or
add a debug_assert!(block_size.is_some()) documenting the invariant.
