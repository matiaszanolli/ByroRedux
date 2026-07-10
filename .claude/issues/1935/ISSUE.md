# WAT-D15-01: Two comments still describe the water-caustic consumer as inactive — it's live in the shipping binaries

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1935

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/vulkan/water.rs:228-229`; `crates/renderer/src/vulkan/context/draw.rs:3683`
**Status**: NEW

## Description
Two comments still describe the water-caustic consumer as inactive: `water.rs` says "Even when Phase D hasn't activated the consumer yet, the descriptor set must exist", and `draw.rs` says "water.frag (Phase D consumer, not yet activated) will atomic-add into it". Both are now false: `water.frag` writes the accumulator and `composite.frag` reads it, in the shipping binaries.

## Evidence
`water.frag.spv` byte-identical to a fresh recompile (contains the `imageAtomicAdd` block); committed `composite.frag.spv` contains the binding-8 `waterCausticTex` read. Runtime call sites confirmed at `draw.rs:1388/3691/479`, `context/mod.rs:2024`.

## Impact
Misleads future maintainers into thinking the path is dead code they can strip — which would silently remove a live rendering feature.

## Related
#1210 (Phase A-E); #1255/#1256/#1257

## Suggested Fix
Reword both comments to state the consumer is live (Phase D/E complete), or drop the "not yet activated" clause entirely.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
