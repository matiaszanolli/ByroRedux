# #3720 — ESM-2026-08-30-D8-01: the "cause not yet identified" FNV LAND 0x00150FC0 failure is an Adler-32 mismatch on an otherwise-perfect zlib stream

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: MEDIUM · **Dimension**: Real-Data Validation
**Record / Sub-record**: `LAND` / `DATA`, `VNML`, `VHGT`
**Location**: `crates/plugin/src/esm/reader.rs` (the `ZlibDecoder … read_to_end` in `read_sub_records`, ~:717-720); soft-fail site `crates/plugin/src/esm/cell/walkers.rs` (~:1027-1044)
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D8-01)

**Status note**: this is the **root cause** for the long-standing note at `walkers.rs` — *"At least one vanilla FNV LAND record (form `0x00150FC0`) reliably fails the body read on every ESM open — cause not yet identified"* (#385 / D5-F5). That comment can be deleted with the fix.

## Description

The record's zlib **Adler-32 trailer is wrong**, not its stream. Raw DEFLATE recovers exactly the declared 4 385 bytes of well-formed `DATA`/`VNML`/`VHGT`.

## Evidence

- The record sits at file offset 185 401 092; `data_size` 4088, `flags` `0x00040000` (compressed); the 4-byte prefix declares **4 385** uncompressed bytes over 4 084 compressed.
- `zlib.decompress` fails with `Error -3: incorrect data check` — an **Adler-32 trailer mismatch**, not a malformed stream.
- Re-inflating the identical payload as raw DEFLATE (`decompressobj(-15)` after the 2-byte zlib header) yields **exactly 4 385 bytes** with `eof == True`, and those bytes parse cleanly as `DATA`(4) + `VNML`(3267) + `VHGT`(1096) — a structurally perfect LAND payload.
- The #3399 ceilings are **not** involved: the ratio ceiling here is 2 091 008 bytes, ~477x the declared size.

**Which cell**: worldspace `TheStripWorldNew`, exterior grid **(-6, 26)**, parent CELL FormID `0x0014F622` — The Strip, visitable content, not a dev cell.

## Impact

One exterior tile of a major FNV location renders with no heightmap and no vertex normals (the flat/untextured symptom the existing comment predicts), visible only at `log::debug`. It is also the **sole non-zero entry in the walker-error column across all seven masters**, so it is the one place where "the ESM parse is clean" is not literally true.

## Suggested Fix

On an inflate error whose recovered output length already equals the validated declared size, accept the buffer and log once at `warn` naming the FormID — a checksum-only failure is exactly recoverable, and Bethesda's own loader evidently tolerates it since the tile renders in the shipped game.

Concretely: keep the `ZlibDecoder` path and, on `Err`, retry with a raw-DEFLATE decoder, accepting only when the byte count matches the declared size. That preserves every #3399 bound. Add a regression fixture built from a valid stream with a corrupted trailer.

Visual confirmation: `TheStripWorldNew` (-6, 26) — see the `/audit-fnv` cross-pointer.

## Completeness Checks
- [ ] **SIBLING**: `byroredux_bsa::safety::inflate_bounded` checked for the same checksum-only failure mode (its contract is mirrored here)
- [ ] **TESTS**: A regression fixture built from a valid stream with a corrupted Adler-32 trailer pins the recovery
