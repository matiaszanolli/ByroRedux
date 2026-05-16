# REN-D8-001: TLAS UPDATE primitive_count violates VUID-03708

**GitHub**: #1083  
**Domain**: renderer (acceleration structures)  
**Severity**: MEDIUM

## Root Cause
tlas.rs line 724: range uses `instance_count` for BOTH BUILD and UPDATE.
BUILD(100) → built with 100 instances. UPDATE(150) → primitive_count=150 ≠ 100 → VUID-03708.

## Fix
Add `built_primitive_count: u32` to TlasState (types.rs).
- BUILD: primitive_count = instance_count; update built_primitive_count
- UPDATE: if instance_count > built_primitive_count → force BUILD; else primitive_count = built_primitive_count
This mirrors BlasEntry's built_vertex_count/built_index_count pattern.

## Files Changed
1. crates/renderer/src/vulkan/acceleration/types.rs (+1 field)
2. crates/renderer/src/vulkan/acceleration/tlas.rs (build logic + range)
