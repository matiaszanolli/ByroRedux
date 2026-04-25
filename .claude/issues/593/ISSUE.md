# FO4-DIM2-02: arraySize = 1 in synthesized DX10 header for cubemaps (spec says 6)

**Severity:** LOW | bsa, import-pipeline
**Source:** `docs/audits/AUDIT_FO4_2026-04-23.md` Dim 2
**Location:** `crates/bsa/src/ba2.rs:572`

## Problem
Synthesized DDS_HEADER_DXT10 hardcodes `arraySize = 1` regardless of cubemap flag. Microsoft spec: cubemap requires `arraySize = 6 × N`. DXGI loaders reject "arraySize must be a multiple of 6"; in-engine renderer is lenient (reads miscFlag).

## Fix
One-line: `let array_size: u32 = if is_cubemap { 6 } else { 1 };`. Lock as unit test.
