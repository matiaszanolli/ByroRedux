# Issue #340: SI-05: Per-frame StringPool.get() lowercase allocation in animation hot path

**Domain**: animation / string interning
**Severity**: LOW (performance)
**Games Affected**: All

## Problem
`StringPool::get()` calls `s.to_ascii_lowercase()` on every lookup, allocating a heap String.
Animation hot path does 60-160 lookups per entity per frame → 300K-600K allocs/sec at scale.

## Fix
Pre-intern channel names as `FixedString` at clip load time. Store AnimationClip channels as
`HashMap<FixedString, TransformChannel>` instead of `HashMap<Arc<str>, TransformChannel>`.
