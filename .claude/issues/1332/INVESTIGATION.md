# #1332 — No parse-block coverage regression pin (block-count parity / NiUnknown ceiling)

## What was missing
Neither existing harness pins parse-block coverage:
- `translation_completeness.rs` measures Material *translation*, never block counts.
- `per_block_baselines.rs` pins per-type `NiUnknown` *growth* (a regression delta),
  not an absolute coverage gate, and doesn't assert block-count parity.

So sizeless-cascade truncations (Oblivion, where a block with no dispatch arm drops the
rest of the file but the file-level recoverable-rate stays 100%) land silently.

## New pin: `crates/nif/tests/block_coverage_baselines.rs`
A distinct surface (kept separate from `translation_completeness` per the
CANONICAL-BOUNDARY check), opt-in `#[ignore]` like the other real-data tests.

- **Oblivion (sizeless): block-count parity, regression-gated.** Walk the mesh archive;
  a truncated parse is `scene.truncated || scene.dropped_block_count > 0`. The set of
  truncating files is checked in (`oblivion_truncations.tsv`); the gate is
  *no-new-truncation* (improvements silent, a newly-truncating file fails). An F-01-class
  regression (sizeless dispatch arm removed) re-truncates files absent from the baseline → red.
- **Sized games (FO3+): NiUnknown-rate ceiling.** `block_size` recovery keeps the block
  count whole, so the signal is the *rate* of `NiUnknown` placeholders, pinned per game.

## Key finding — premise was partly stale, plus a broader gap
- F-01 (NIF-2026-05-29-01) is **already fixed**: `bhkConvexSweepShape` / `bhkMeshShape`
  have dispatch arms (`blocks/mod.rs:1107-1108`); the three named files
  (`handscythe01.nif` / `oar01.nif` / `ungrdltraphingedoor.nif`) parse whole now. The
  pin is correctly *green* for them ("green after the HIGH fix").
- **But 55 vanilla Oblivion meshes still truncate** from *other* undispatched sizeless
  block types (e.g. `summondaedra.nif` −107, `tumbler.nif` −102, creature heads, markers).
  A hard `parsed==num_blocks` gate would be red on all 55; fixing them means writing new
  block parsers — out of scope for a coverage-pin. They are now the visible, tracked
  contents of `oblivion_truncations.tsv` rather than a silent loss. **Follow-up worth a
  separate issue: dispatch/skip the remaining 55-file Oblivion sizeless-truncation tail.**

## Baselines captured (all 7 games' data present on the dev machine)
| Game | total blocks | NiUnknown | rate |
|------|-------------:|----------:|-----:|
| Oblivion | — | — | 55 truncating files / 8032 |
| Fallout 3 | 287,331 | 0 | 0% |
| Fallout NV | 492,796 | 0 | 0% |
| Skyrim SE | 665,846 | 0 | 0% |
| Fallout 4 | 740,562 | 0 | 0% |
| Fallout 76 | 1,548,202 | 0 | 0% |
| Starfield | 770,322 | 1,036 | 0.1345% |

## Verification
- Regen → run: all 7 tests green against checked-in baselines.
- F-01 files confirmed absent from `oblivion_truncations.tsv` → an F-01 regression
  surfaces as a NEW truncation (red).
- Default `cargo test` unaffected (tests are `#[ignore]`d).
