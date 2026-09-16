# #4419 — RT-2026-09-16-03: The skill's `light_count_point` definition disagrees with the committed Oblivion baseline

**Labels**: low,documentation,doc-rot

**Source**: `docs/audits/AUDIT_RUNTIME_2026-09-16.md` (RT-3)

- **Severity**: LOW
- **Status**: NEW
- **Dimension**: audit infrastructure (SKILL.md Phase 3 / doc rot)
- **Description**: Phase 3 says to parse `light_count_point` from the
  `LightSource emitters: N` tally. That tally counts **every** emitter,
  directional ones included. Oblivion's dump today is `emitters: 10`: 8
  `kind=Point` rows plus 2 `kind=Directional` rows. The baseline stores
  `light_count_point 8`, which is the count of `kind=Point` rows. Following
  the text literally produces a false 8→10 exact-match failure. The other four
  cells have 0 directional emitters, so the two definitions only disagree on
  Oblivion.
- **Related doc rot in the same section**:
  - *"every one dumps `directional_color = [0.000, 0.000, 0.000]`"* no longer
    holds. FNV's `CellLightingRes` dumps `directional_color = [0.224, 0.208, 0.133]`.
  - The *Checked-in baselines* table is stale on three cells:

    | Cell | Table says | TSV says |
    |------|------------|----------|
    | Oblivion | 705 | 745 |
    | FO4 | 19 399 | 18 969 |
    | FNV | "7 342" | 7342 ✓ |

- **Suggested Fix**: Define `light_count_point` as the count of `kind=Point`
  rows, the same rule `light_count_directional` already uses. Drop the
  zero-directional-colour claim, and point the table at the TSVs instead of
  repeating their numbers.

## Completeness Checks
- [ ] **SIBLING**: `light_count_directional` and `light_count_point` use the same row-count rule
