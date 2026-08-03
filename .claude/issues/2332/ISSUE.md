# FO3-D5-02: bhkSPCollisionObject classified CollisionAuthoring::Classic although it's a phantom wrapper

Filed from: `docs/audits/AUDIT_FO3_2026-08-03.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2332

**Severity**: LOW
**Location**: `crates/nif/src/import/collision/mod.rs:69-85`; dispatch `crates/nif/src/blocks/mod.rs:1139-1141`
**Status**: NEW

### Description
The dispatcher byte-wise-correctly folds `bhkSPCollisionObject` into `BhkCollisionObject`, but nif.xml declares it inherits `bhkPCollisionObject` (a phantom wrapper). Because the parsed struct is `BhkCollisionObject`, `examine_collision_kind` reports `Classic` and routes it into `extract_from_classic`, which fails to resolve the phantom body and returns `None` via the generic debug log rather than the deliberate `extract_from_phantom` path.

Confirmed against current code: `blocks/mod.rs:1139-1141` — `"bhkCollisionObject" | "bhkSPCollisionObject" => Ok(Box::new(BhkCollisionObject::parse(stream, false)?))` — both dispatch to the same struct. `import/collision/mod.rs:75` (`examine_collision_kind`) checks `block.as_any().is::<BhkCollisionObject>()` → `CollisionAuthoring::Classic`, never reaching the `BhkPCollisionObject` → `Phantom` arm for `bhkSPCollisionObject` blocks.

### Evidence
Live scan of the 5 FO3 DLC archives: `bhkSPCollisionObject` 25 parsed/0 unknown, and exactly 25 of 10,297 `BhkCollisionObject`-typed blocks fail extraction — matching the 25 `bhkSimpleShapePhantom` blocks 1:1.

### Impact
No runtime behavior difference today (both paths yield `None`) — diagnostic-only. A future `TriggerVolume` importer keying off `CollisionAuthoring::Phantom` would silently skip these 25 blocks.

### Suggested Fix
Give `bhkSPCollisionObject` its own dispatch arm producing `BhkPCollisionObject`.

### Related
FO3-D5-03

## Completeness Checks
- [ ] **SIBLING**: Shared code path with FNV — same misclassification applies there
- [ ] **TESTS**: A regression test pins "bhkSPCollisionObject classifies as CollisionAuthoring::Phantom, not Classic"
