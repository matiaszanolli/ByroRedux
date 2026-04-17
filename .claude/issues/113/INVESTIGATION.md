# #113 / NIF-13 — Unbounded allocation in `stream.read_bytes`

## Root cause

`NifStream::read_bytes(len)` allocated `vec![0u8; len]` unconditionally. The callers pass `len` derived from u32 fields parsed out of the NIF, so a corrupt or malicious file could claim an arbitrary size and force the parser to allocate gigabytes before the subsequent `read_exact` fails. Same pattern on every bulk array reader (`read_ni_point3_array`, `read_ni_color4_array`, `read_uv_array`, `read_u16_array`, `read_u32_array`, `read_f32_array`), where `count` is attacker-controlled.

Callers of `read_bytes` that currently pass file-driven lengths:

- `extra_data.rs:69` — `NiBinaryExtraData::binary_data`
- `collision.rs:164` — `BhkSystemBinary` (FO4 NP physics blob, just added in #124)
- `collision.rs:638` — bhkMoppBvTreeShape MOPP payload
- `stream.rs:324` — pre-20.1 inline string
- `stream.rs:345, 357` — sized/short strings

The issue title mentions `mod.rs:264-275` — that particular site now uses `stream.skip(size)` which doesn't allocate, but the class of bug it described is real on the six sites above. This fix covers all of them via the central reader.

## Fix

Added a public `MAX_SINGLE_ALLOC_BYTES` constant (256 MB) and a private `check_alloc` helper that validates:

1. `bytes <= MAX_SINGLE_ALLOC_BYTES` — hard cap well above any legitimate single-block payload.
2. `bytes <= remaining stream bytes` — physically impossible claims fail immediately with `UnexpectedEof` so the existing block-size recovery path can swallow the error.

Every allocating reader now calls `check_alloc(byte_count)` before `vec![0u8; byte_count]`:

- `read_bytes`
- `read_ni_point3_array`
- `read_ni_color4_array`
- `read_uv_array`
- `read_u16_array` / `read_u32_array` / `read_f32_array`

Callers that route through `read_bytes` (strings, binary extra data, Havok blobs) inherit protection transparently.

## Error semantics

- **Oversized vs. remaining stream** → `UnexpectedEof`. This is the existing error code for "ran off the end" and the block-size recovery loop in `lib.rs` already handles it, so corrupt blocks are swallowed the same way an early EOF would be.
- **Over the hard cap** → `InvalidData`. A legitimate file should never hit this; failing loud surfaces the corruption without letting the file truncate parsing silently.

## Regression tests

Three new tests in `stream::tests`:

- `read_bytes_oversized_request_errors_before_alloc` — 64-byte buffer, request 1 MB, must fail with `UnexpectedEof` and leave the cursor untouched.
- `read_bytes_over_hard_cap_errors_regardless_of_stream_size` — 257 MB buffer, request 257 MB, must fail with `InvalidData` (the cap fires before the remaining check would).
- `read_bytes_at_cap_succeeds` — request equal to remaining bytes must still pass; the cap is inclusive.
