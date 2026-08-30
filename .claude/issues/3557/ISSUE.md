# RT-11: Oblivion emits two byte-identical synthetic `__max_default_light` directional emitters — double the intended synthetic contribution

**Issue**: #3557
**Labels**: bug, renderer, low, game:oblivion
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-30.md`

---

Source: `docs/audits/AUDIT_RUNTIME_2026-08-30.md` — RT-11.

## Description

Oblivion / `ICMarketDistrictTheGildedCarafe` emits **two byte-identical synthetic directional light emitters**, both named `__max_default_light` (a 3ds Max default-light node carried through in the exported NIF).

## Evidence

Entities `142` and `143`, from the live per-emitter dump:
```
name="__max_default_light" kind=Directional source=nif/synthetic (no FormId ancestor)
direction=[0.8947, 0.3716, 0.2478]
radiant=[1,1,1]  dimmer=1.000  range_m=58.514
legacy_flags=0x00001000
```
Both rows are identical in every field. Oblivion is the **only** game of the five captured that synthesises a directional light at all in an interior.

## Impact

An interior lit by two stacked full-intensity directionals receives **double** the intended synthetic contribution. This is very likely why oblivion is the one cell whose emitter total drifted from the skill's recorded value (8 -> 10), and it is the residual that RT-10's oblivion row cannot otherwise explain.

## Suggested Fix

De-duplicate synthetic default lights **by name** at import, or hoist them to one per scene rather than one per contributing NIF. The per-NIF-import path is the multiplier: two NIFs in the cell each carrying a `__max_default_light` node yield two emitters.

## Completeness Checks
- [ ] **SIBLING**: Other well-known exporter-artifact light node names checked for the same duplication (not just `__max_default_light`)
- [ ] **CANONICAL-BOUNDARY**: The de-dup happens once at the import/translate boundary — not re-derived per frame in `byroredux/src/render/lights.rs`
- [ ] **TESTS**: A regression test pins that two NIFs contributing the same synthetic default light yield one emitter
