# SAFE-D2-01: Header-parser read_sized_string allocates unbounded (residual #388 gap)

**Issue**: #1903 · **Severity**: MEDIUM · **Labels**: medium, nif-parser, safety, bug
**Dimension**: Memory Corruption / UB (unbounded allocation on malformed input) · **Filed from**: docs/audits/AUDIT_SAFETY_2026-07-06.md (nif-deep suite)
**Location**: crates/nif/src/header.rs:387-395 (read_sized_string); reached from header.rs:192
(block-type names) and header.rs:266 (string-table entries)

## Description
#388 OOM-hardening added MAX_SINGLE_ALLOC_BYTES (256MB) + remaining-bytes guards via
NifStream::check_alloc — only to the stream BODY. The header parser runs before NifStream exists and
reads inline strings via read_sized_string, which does `vec![0u8; len]` (header.rs:389) with len an
untrusted u32 — no cap, no remaining check. Count guards (num_block_types, num_strings) bound HOW MANY
strings, not each string's LENGTH. A corrupt entry can claim len=0xFFFFFFFF → single ~4GB zeroing alloc
before read_exact fails. (read_short_string, header.rs:397, uses u8 len → max 255 → not a concern.)

## Impact
Crafted/corrupt NIF header → transient ~4GB alloc per offending string on all 7 games' load paths.
Bounded at u32, fails cleanly → not UB, but can OOM/abort on a memory-constrained host. Malformed-input
DoS / defense-in-depth gap; the stream body already guards this exact class.

## Suggested Fix
Thread the same budget check into the header readers — a check_header_alloc(len, remaining) helper
mirroring check_alloc, or reject len > remaining inline before vec![0u8; len].

**Related**: #388 (CLOSED, stream-side); #113 (alloc-cap origin).
