# #126 / NIF-206 — NiSkinPartition SSE gate off by exact-match

## Root cause

`NiSkinPartition::parse` gates two SSE-specific field groups on
`stream.bsver() == 100`:

- global prefix (line 185): `data_size: u32 + vertex_size: u32 + vertex_desc: u64` + optional byte blob
- per-partition trailer (line 302): `vertex_desc: u64 + triangles_copy[num_triangles * 6]`

`NifVariant::detect` in `version.rs:102` maps `(user_version=12, user_version_2 < 130)` to `SkyrimSE`, so real-world files with BSVER between 101 and 129 are classified as SSE but their actual `stream.bsver()` is 101–129 (the raw `user_version_2` — we don't rewrite headers). The exact-match gate dropped the SSE fields for every minor above 100, desynchronising the stream on the first partition read.

## Games affected

SkyrimSE files that use `user_version_2` outside the canonical 100. Vanilla SSE content uses 100 (nominal), but CK-regenerated assets and some mod tooling produce 101+ — the audit flagged it as a hypothetical the classifier already accommodates.

## Fix

- Both sites now gate on `let is_sse = (100..130).contains(&bsver);` — exactly the range `detect()` folds into `SkyrimSE`.
- `is_sse` is computed once at the top of `parse` so the inner loop shares the bool.

## Sibling check

`grep -rn "bsver == 100"` across the tree returns these two sites only. No other block parser gates SSE-specific behavior on exact bsver equality.

## Regression tests

Three new tests in `blocks::skin::tests`:

- `skin_partition_sse_100_consumes_sse_fields` — canonical BSVER=100 exercises the baseline.
- `skin_partition_sse_105_consumes_sse_fields` — representative value inside the `[101, 130)` gap that previously dropped the SSE fields.
- `skin_partition_sse_129_consumes_sse_fields` — upper boundary of the range.

Each builds a minimal one-partition payload (zero vertices / triangles / bones / strips) that only consumes the stream exactly when the SSE branch fires for the global prefix AND the per-partition trailer. Any regression narrowing the gate back to `== 100` fails the 105/129 tests immediately.
