title:	NIFAL-D7-03: operation->FloatTarget and target_color->ColorTarget discriminator tables duplicated between KF and embedded animation arms
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	animation, bug, low, tech-debt
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	2304
--
**Severity**: LOW
**Dimension**: Animation · **Tier Violated**: single-boundary (secondary)
**Location**: `crates/nif/src/anim/channel.rs:296-301,383-388` (KF arm) vs. `crates/nif/src/anim/entry.rs:358-365,378-383` (embedded arm)
**Status**: NEW

## Description

The `operation`→`FloatTarget` and `target_color`→`ColorTarget` discriminator
tables are duplicated between the KF arm (`channel.rs`) and the embedded arm
(`entry.rs`). Byte-identical today. Not a duplicate of `#2067` (that issue
tracks a different duplication — the `NiSingleInterpController` prologue
reimplemented at 4 sites).

## Evidence

```rust
// channel.rs and entry.rs, both:
match ctrl.target_color {
    1 => ColorTarget::Ambient,
    2 => ColorTarget::Specular,
    3 => ColorTarget::Emissive,
    _ => ColorTarget::Diffuse,
}
```
and the matching `operation` → `FloatTarget::UvOffsetU/V`/`UvScaleU/V`/`UvRotation` table.

## Impact

Latent drift risk only — both copies agree today. A future new
`FloatTarget`/`ColorTarget` variant added to one table without the other
would silently diverge KF vs. embedded-controller behavior for the same NIF
controller type.

## Suggested Fix

Extract both discriminator tables into shared functions (e.g.
`float_target_from_operation(u32) -> FloatTarget`,
`color_target_from_target_color(u32) -> ColorTarget`) and call them from both
arms.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (both KF and embedded arms)
- [ ] **TESTS**: A regression test pins this specific fix

