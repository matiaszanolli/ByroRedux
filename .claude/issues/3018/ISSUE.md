# SCR-D8-03: one out-of-range annotation timestamp hard-fails the entire clip decode

**Issue**: #3018
**Severity**: LOW
**Dimension**: 8 — Havok packfile reader
**Labels**: `low,animation,bug`
**Source report**: `docs/audits/AUDIT_SCRIPTING_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-16.md` (Dimension 8 — Havok packfile reader).

**Location**: `crates/hkx/src/animation.rs`:331-333, contrast :341

## Description

One out-of-range annotation timestamp **hard-fails the entire clip decode**, while the very next lines show the codebase already knows how to tolerate the same condition by clamping.

## Evidence

```rust
// :331-333 — hard fail
let time = pack.f32(annotation, "annotation time")?;
if !time.is_finite() || time < 0.0 || time > duration + 0.001 {
    return Err(HkxError::InvalidData("annotation time is out of range"));
}
```

```rust
// :341 — …and then the accepted value is clamped anyway
time: time.min(duration),
```

Re-verified 2026-08-17. The clamp at :341 makes the strict rejection at :333 redundant for any value the check would have accepted — the two policies disagree about what an out-of-range time means.

## Impact

A single bad annotation timestamp anywhere in a clip discards the whole animation, including all its transform tracks. Annotations are text-event metadata; losing the clip over one is disproportionate.

LOW because vanilla data is well-formed — but the crate reads untrusted archive input, and the inconsistency means the failure mode is arbitrary rather than designed.

## Suggested Fix

Pick one policy. Given `:341` already clamps, the consistent choice is to skip or clamp the offending annotation and keep the clip, logging the anomaly — reserving `InvalidData` for structural corruption rather than one out-of-range float.

## Related

- #3011, #3013, #3014 — same crate's other parser-discipline findings

## Completeness Checks
- [ ] **ONE-POLICY**: The reject-vs-clamp inconsistency between :333 and :341 is resolved, not left as two rules
- [ ] **PROPORTIONATE**: A metadata anomaly no longer discards transform tracks
- [ ] **SIBLING**: Other hard-fail arms in the annotation/text-event path reviewed for the same disproportion
- [ ] **TESTS**: A fixture with one out-of-range annotation still yields a usable clip

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3018 --json state` when live state is needed.*
