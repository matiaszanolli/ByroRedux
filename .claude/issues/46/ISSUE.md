# Perf: transform propagation acquires 4 locks per BFS node (PERF-04)

**Severity:** HIGH | Dimension: ECS Query Patterns
**Labels:** ecs, performance
**Source:** Performance Audit 2026-04-04

## Finding
BFS loop in `transform_propagation_system` (main.rs:476-513) acquires/drops Parent, GlobalTransform(r), Transform, GlobalTransform(w) locks **per node**. 500 entities = 2000+ atomics/frame.

## Fix
Hold all four locks for entire BFS. Pre-compute topological order for linear pass.
