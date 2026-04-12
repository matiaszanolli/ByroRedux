# #262: LC-02 — NiGeomMorpherController morph index hardcoded to 0

**Severity**: MEDIUM | **Domain**: animation, legacy-compat | **Type**: bug
**Location**: `crates/nif/src/anim.rs:351`

## Problem
Only MorphWeight(0) emitted. Multi-target facial animation collapses all weights to index 0.

## Fix
Iterate morph target interpolators, emit MorphWeight(i) per target.
