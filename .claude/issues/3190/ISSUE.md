# #3190 — SPT-D3-2026-08-20-01: SpeedTreeWind is built from two CNAM floats whose meaning the parser documents as unpinned

- **Filed**: 2026-08-20 (`/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3190
- **Labels**: `medium,legacy-compat,bug`
- **Source report**: `docs/audits/AUDIT_SPEEDTREE_2026-08-20.md`
- **HEAD at audit**: `bb0b92f2`

---

**Severity**: MEDIUM
**Dimension**: TREE→Billboard Wiring (secondary: Placeholder Fallback)
**Source**: `docs/audits/AUDIT_SPEEDTREE_2026-08-20.md` (`SPT-D3-2026-08-20-01`) — HEAD `bb0b92f2`

## Location

- `byroredux/src/cell_loader/references/import.rs` — the `CNAM` → `(response, stiffness)` mapping
- `crates/plugin/src/esm/records/tree.rs` — the `CNAM` module docstring and the `canopy_params` field doc
- `byroredux/src/systems/billboard.rs` — `apply_speedtree_wind`, the `bend` expression
- `crates/core/src/ecs/components/billboard.rs` — `SpeedTreeWind`

## Status

NEW — introduced by `4ddf7062`.

## This is a No-Guessing Policy violation

The project's standing rule is: **never guess values or heuristics; research docs, source code, papers
first; ask for documentation if it is missing.** This delta shipped a vegetation feature whose entire
input is a positional guess about a record layout that the repository's own parser documents as
**unpinned**, and whose cited upstream reference does not decode the field at all.

## Description

The delta added a wind response sourced straight off the TREE record's `CNAM` array:

```rust
let response  = values.next()?.max(0.0);          // canopy_params[0]
let stiffness = values.next()?.clamp(0.0, 1.0);   // canopy_params[1]
```

consumed as `bend = strength * 0.16 * response * (1.0 - stiffness)` in `apply_speedtree_wind`.

Nothing in the repository establishes that `CNAM[0]` is a wind-response multiplier or that `CNAM[1]` is
a normalised stiffness. **The record's own module docstring says the opposite, twice:**

> `CNAM` — canopy shadow / wind parameters as a contiguous f32 array. Field count varies per game
> (5 floats Oblivion, 8 floats FO3/FNV); **semantics not pinned down here** — we surface the raw values
> for the future SpeedTree runtime to interpret.

The parser's cited upstream reference, OpenMW's `components/esm4/loadtree.cpp`, puts `CNAM` in the
`skipSubRecordData()` arm (verified at `/mnt/data/src/reference/openmw/components/esm4/loadtree.cpp`) —
`loadtree.hpp` stores only `mEditorId`, `mModel`, `mBoundRadius`, `mLeafTexture`. **It supplies no field
layout either.** The mapping was derived from field *position*, not from a documented layout.

### The guess is not neutral — the clamp silences the feature

`stiffness` is `.clamp(0.0, 1.0)` and enters the bend as `(1.0 - stiffness)`. **Any tree whose
`CNAM[1]` is ≥ 1.0 gets `bend == 0` — no sway at all**, silently, with no diagnostic. If `CNAM[1]` is an
angle in degrees, a count, or a dimming value (all plausible for a "canopy" struct), that is every tree
in the game.

Symmetrically, `response` is clamped to `[0, 4]`, so a `CNAM[0]` of 4.0 with `CNAM[1]` of 0.0 yields
`bend = 0.64 rad` — a **±37° trunk swing**, which is not a tree, it is a windscreen wiper. A single
record field, read from an unpinned slot, spans "completely inert" to "grossly wrong" with nothing in
between.

### Secondary defect in the same expression: the field indices are data-dependent

`.filter(|v| v.is_finite())` runs **before** `.take(2)`, so a non-finite `CNAM[0]` silently promotes
`CNAM[1]` to `response` and `CNAM[2]` to `stiffness` — the mapping *shifts* rather than rejecting the
record.

## Evidence

**The repo's own fixtures disprove the mapping.** `crates/plugin/src/esm/records/tree.rs` carries two
CNAM value sets, and **both** land in the "feature does nothing" regime:

| Fixture | `CNAM` | Derived `stiffness` | Resulting `bend` |
|---|---|---|---|
| FNV — docstring: *"Realistic FNV TREE record … CNAM carries 8 floats. This is the **modal vanilla shape** across `FalloutNV.esm`"* | `[0.5, 1.0, 0.7, 2.5, 1.2, 0.3, 0.4, 1.0]` | `1.0` | **0 — no sway** |
| Oblivion | `[0.4, 0.9, 0.6, 1.8, 1.0]` | `0.9` | 90 % of the response thrown away |

The only end-to-end test, `parse_and_import_spt_preserves_tree_cnam_wind_response`
(`byroredux/src/cell_loader/references/import_tests.rs`), hand-picks `canopy_params: vec![2.0, 0.25]` —
values chosen so the feature works, **not drawn from game data**. Every `billboard.rs` test likewise
constructs `SpeedTreeWind::new(1.0, 0.0)` directly, so nothing exercises the
`canopy_params` → `SpeedTreeWind` edge against real content.

## Impact

The delta's headline vegetation feature is driven by misattributed record data across FNV / FO3 /
Oblivion — the three games that ship `.spt` trees at all. Depending on what `CNAM[1]` actually holds,
either the entire canopy response is inert on vanilla content (and the eight billboard commits bought
nothing visible), or trees swing at up to ±37°. **Neither failure produces a log line.** This is a
compatibility-data correctness defect, not a crash.

## Suggested Fix — find a real source, do not tune the constant

**Do not infer the layout, and do not "fix" this by adjusting the clamp or the `0.16` coefficient.** Pin
`CNAM` against a citable source first:

- The TES4 CS / GECK **Tree** dialog exposes the struct field-by-field and is the natural authority.
- A corpus histogram over `FalloutNV.esm` / `Fallout3.esm` / `Oblivion.esm` will disambiguate the
  5-float vs 8-float split and show which slots are bounded, which are angles, which are counts.

Then record the layout in `crates/plugin/src/esm/records/tree.rs`'s docstring and read the **named**
wind fields.

Until that lands, either:

- **(a)** gate `SpeedTreeWind` behind a neutral default `(1.0, 0.0)` for every record and drop the
  `CNAM` read entirely — honest, and visually identical to today on vanilla content; or
- **(b)** **reject rather than clamp**: treat `CNAM[1] > 1.0` as "this slot is not a normalised
  stiffness" and fall back to the default instead of silently producing a rigid tree.

Independently: move `.take(2)` **before** the finiteness filter so the field indices stay positional.
Add a corpus-gated test asserting the derived `(response, stiffness)` distribution over real TREE
records is not degenerate.

## Related

- **#3076** (CLOSED, fix verified at HEAD) — the billboard wiring this rides on.
- **TD5-011** — the parse-but-don't-consume gate that `CNAM` was explicitly *inside* until this delta.
- The project **No Guessing Policy**.

## Completeness Checks

- [ ] **SIBLING**: check whether any other consumer reads `canopy_params` positionally once the layout
      is pinned (`SNAM` leaf indices are the adjacent unpinned field)
- [ ] **CANONICAL-BOUNDARY**: the `CNAM` → wind interpretation stays at the ESM record → import boundary,
      not re-derived in `billboard.rs` or in a shader
- [ ] **TESTS**: a corpus-gated test over real FNV/FO3/Oblivion TREE records asserting the derived
      `(response, stiffness)` distribution is not degenerate — the current test hand-picks working values
- [ ] **DOCS**: `crates/plugin/src/esm/records/tree.rs`'s "semantics not pinned down here" docstring is
      replaced with the cited layout, or left in place and the consumer removed
