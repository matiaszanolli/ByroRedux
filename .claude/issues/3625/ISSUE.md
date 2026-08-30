# #3625: OBL-D4-02: NiTexturingProperty Apply Mode values 1 and 3 are decoded and then dropped (681 Oblivion properties)

**Source**: `docs/audits/AUDIT_OBLIVION_2026-08-30.md` — Dimension 4 (Rendering Path for Oblivion Shaders)
**Severity**: LOW
**Location**: `crates/nif/src/blocks/properties.rs` (`NiTexturingProperty::apply_mode`), `crates/nif/src/import/material/legacy_properties.rs` (the sole consumer)

## Description

`apply_mode` is decoded from both its on-disk homes and then consumed at exactly one site,
for exactly one value (`APPLY_HILIGHT2`, 4). Values 1 (`APPLY_DECAL`) and 3
(`APPLY_HILIGHT`) are decoded and dropped.

## Evidence

Verified 2026-08-30 — the only read of the field outside the parser is:

```
crates/nif/src/import/material/legacy_properties.rs: if tex_prop.apply_mode == APPLY_HILIGHT2 && ...
```

Measured apply-mode histogram over 30,121 Oblivion `NiTexturingProperty` instances:

| value | name | count |
|---|---|---|
| 1 | `APPLY_DECAL` | 18 |
| 2 | `APPLY_MODULATE` (default) | 28,166 |
| 3 | `APPLY_HILIGHT` | 663 |
| 4 | `APPLY_HILIGHT2` | 1,274 |

## Impact

681 properties (663 + 18) carry a non-default blend intent the renderer never sees. The
magnitude is small and the semantics are genuinely uncertain — value 3 is documented in the
parser as "PS2 only", and Gamebryo v3.2 renamed both 3 and 4 to
`APPLY_DEPRECATED`/`APPLY_DEPRECATED2`.

## Suggested Fix

**No heuristic is proposed and none should be invented** (project no-guessing policy). This
issue records the measurement so a future decision has a number attached: either establish
the semantics from a primary source before consuming values 1 and 3, or document them as
deliberately dropped at the field's doc comment.

## Related

#3530 / OBL-D4-01 — the `APPLY_HILIGHT2` consumer, which is itself currently inert on
vanilla Oblivion.

## Completeness Checks
- [ ] **TESTS**: if either value is later consumed, a regression test pins the decode and the downstream material effect
