Surfaced 2026-05-28 in the first Cydonia render attempt (Phase 5 of [docs/engine/starfield-esm-roadmap.md](docs/engine/starfield-esm-roadmap.md)).

## Symptom

Loading `Starfield.esm` with `--cell citycydoniamainlevel` produces 11 985 of these warnings — one per interior cell in the ESM:

```
WARN  byroredux_plugin::esm::cell::walkers
ESM XCLL with 108 bytes is non-canonical for Starfield (expected one of [28, 40]);
the size-based dispatch will read whatever extended fields fit but cell lighting
may be mis-computed.
```

11 985 matches the exact interior-cell count `sf_parse_check` reported for `Starfield.esm`. **Every interior cell in vanilla Starfield trips this warning.**

## Root cause

`crates/plugin/src/esm/cell/walkers.rs:49-58`:

```rust
fn xcll_canonical_sizes(game: GameKind) -> &'static [usize] {
    match game {
        GameKind::Oblivion => XCLL_SIZES_OBLIVION,
        GameKind::Fallout3NV
        | GameKind::Fallout4
        | GameKind::Fallout76
        | GameKind::Starfield => XCLL_SIZES_FALLOUT_ERA,  // = &[28, 40]
        GameKind::Skyrim => XCLL_SIZES_SKYRIM,
    }
}
```

Test at `walkers.rs:73-88` explicitly asserts `Starfield = [28, 40]` — derived from FNV/FO3/FO4 baseline. Starfield actually authors **108 bytes**:

1. Trips the sanity-warn on every cell.
2. Reads "whatever extended fields fit" — likely the first 40 bytes as FO4-style XCLL, then 68 bytes of authored SF extended fields silently dropped.

The runtime `byro-dbg` `light.dump` against `citycydoniamainlevel` DOES report a populated 6-axis directional ambient cube + `fog_clip / fog_power / fog_far_color / fog_max / directional_fade` — so the handler decodes SOMETHING, but it's reading fields out of the wrong offsets relative to the 108-byte layout.

## Fix

1. Split Starfield off its own size set: `XCLL_SIZES_STARFIELD: &[usize] = &[28, 40, 108]` (verify against `BlueprintShips-Starfield.esm` + `ShatteredSpace.esm` to confirm 108 isn't a one-off).
2. Update `xcll_canonical_sizes(GameKind::Starfield) => XCLL_SIZES_STARFIELD`.
3. Add `xcll_starfield_sizes_pinned` test (mirror of `fallout_era_xcll_sizes_pinned`).
4. **Investigate the 108-byte layout** — what new fields did Bethesda add post-FO76? Likely candidates: new HDR exposure params, additional fog curve points, the directional ambient cube format change.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: confirm no other size-based cell-subrecord tables also assume FO4-baseline for Starfield (XCLW water type? XCLR climate? XEZN encounter zone?)
- [ ] **TESTS**: pin the new size set + extend `xcll_gate_tests`
- [ ] **REGRESSION**: re-run `sf_parse_check` + assert zero "non-canonical for Starfield" warnings

## References

- Cydonia render: [docs/engine/starfield-esm-phase0-baseline.md](docs/engine/starfield-esm-phase0-baseline.md)
- Walker: `crates/plugin/src/esm/cell/walkers.rs:33-58`
- Test pin: `crates/plugin/src/esm/cell/walkers.rs:73-88`
