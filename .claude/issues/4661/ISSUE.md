# PAR-D1-2026-09-21-07: Entry-count reservations are absolute rather than file-relative, and BSA's declared file_count is never reconciled with its folder records

Labels: low,bug,import-pipeline

## Description
`crates/bsa/src/archive/open.rs:155`, `:200`, `:320`, `crates/bsa/src/ba2.rs:302`, `:323`, `:502`, `:543`, `crates/bsa/src/csg.rs:168` all size a `Vec`/`HashMap` with `Vec::with_capacity(count)` / `HashMap::with_capacity(count)` where `count` was clamped only by `checked_entry_count`'s absolute `MAX_ENTRY_COUNT` (10,000,000), not by anything derived from the remaining bytes in the file.

A BSA's header `file_count` is also independent of the sum of its folder records' `count` fields, and is never cross-checked — a 36-byte BSA declaring 10M files and 0 folders opens `Ok` with 0 files. CSG already holds `file_len` in scope when it sizes `vec![0u8; num_chunks * 8]` but doesn't use it to bound `num_chunks` relative to the file.

Verified unchanged at HEAD `ee6d3fb39`: all cited reservation sites still size off the `checked_entry_count`-capped count alone.

## Evidence
Probes `bsa-reserve` / `ba2-reserve`:

```
36-byte BSA (10M files / 0 folders): open -> Ok (0 files) in 5.5 ms; VmHWM 3,428 kB -> 19,936 kB (+ ~1.2 GB untouched virtual)
24-byte BA2 (10M files): open -> Err("failed to fill whole buffer") in 50 us; VmHWM flat
```

## Impact
Harmless on Linux overcommit (only the HashMap control bytes are touched in the observed case). The reservation is committed charge on Windows, where overcommit works differently. The error the BA2 case does return carries no field context (offset, declared count).

## Related
#586 (the entry-count cap this reservation still relies on), #2614 (the sfmaterial `count.min(bytes.len() / 8)` clamp is the in-repo pattern to follow)

## Suggested Fix
Clamp each capacity hint to `remaining_file_bytes / record_size`, and warn (or `Err`) when a BSA's `file_count != sum(folder.count)`.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D1-2026-09-21-07)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix