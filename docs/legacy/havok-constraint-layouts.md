# Havok Constraint NIF Serialization Layouts

Reference: Havok SDK 2007.09.19 + nif.xml cross-reference.
For M28 (Physics) when implementing full constraint parsing.

## Common Prefix (bhkConstraintCInfo)

All constraints: `num_entities(u32) + entity_a(i32) + entity_b(i32) + priority(u32)` = **16 bytes**

## Constraint Types

### bhkBallAndSocketConstraint (type=0) — 48 bytes
- pivotA(vec4) + pivotB(vec4) = 32 bytes

### bhkHingeConstraint (type=1) — Oblivion 96, FO3+ 144 bytes
- Oblivion: 5 × vec4 = 80 bytes
- FO3+: 8 × vec4 = 128 bytes (reordered as hkTransform pairs)

### bhkLimitedHingeConstraint (type=2) — Oblivion 140, FO3+ 156+ bytes
- Oblivion: 7 × vec4 + 3 floats = 124 bytes
- FO3+: 8 × vec4 + 3 floats + motor(variable) = 140+ bytes

### bhkPrismaticConstraint (type=6) — 156+ bytes
- 8 × vec4 + 3 floats = 140 bytes (both versions, different order)
- FO3+: + motor(variable)

### bhkRagdollConstraint (type=7) — Oblivion 136, FO3+ 168+ bytes
- Oblivion: 6 × vec4 + 6 floats = 120 bytes
- FO3+: 8 × vec4 + 6 floats + motor(variable) = 152+ bytes

### bhkStiffSpringConstraint (type=8) — 52 bytes
- pivotA(vec4) + pivotB(vec4) + length(f32) = 36 bytes

### bhkMalleableConstraint (type=13) — recursive wrapper
- base(16) + type(u32) + inner_cinfo(16) + inner_constraint_data(polymorphic)
- Oblivion: + tau(f32) + damping(f32)
- FO3+: + strength(f32)

## bhkConstraintMotorCInfo (FO3+ only, variable-length)
- type(u8): 0=none(0), 1=position(25), 2=velocity(18), 3=spring-damper(17) bytes

## Key Insight
NIF files serialize bhk*CInfo parameters, NOT Havok atom chains.
Bethesda's bhk* wrappers reconstruct atoms at load time.
Oblivion vs FO3+ have completely different axis ordering per constraint type.
