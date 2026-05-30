# #1305 (OBL-D6-NEW-02) Investigation — audit premise invalidated by data

## Finding (as filed)
"Tamriel ocean never renders — worldspace-default water (NAM2/DNAM) has no
cell-level fallback." Suggested fix: **parse WRLD DNAM (default land + water
height, 2× f32)**; resolve worldspace `water_form`→WATR + default height in
`build_exterior_world_context`; fall back per-cell in `load_one_exterior_cell`.

## Data verification against real Oblivion.esm (277 MB)

| Check | Result |
|---|---|
| WRLD records | 84 |
| WRLD with NAM2 (water_form) | 54 |
| **WRLD with DNAM** | **0** |
| CELL records | 35494 |
| CELL with XCLW (explicit water height) | 751 (2%) |
| Most common XCLW value | `-2147483648.0` (170×) — Bethesda "no water" sentinel |
| Next XCLW values | -2000 (168×), 3000 (74×), 100 (50×), -4000 (25×) |

## Conclusion: the suggested fix is wrong for Oblivion

- **DNAM does not exist on Oblivion WRLD.** "Default land/water height in WRLD
  DNAM" is a Skyrim+ (Land Data) mechanism. Parsing DNAM would be dead code on
  Oblivion. (The audit imported the Skyrim/FO4 worldspace layout onto Oblivion.)
- **98% of Oblivion cells have no XCLW** and rely on a worldspace default whose
  height source is NOT DNAM and is not yet identified. Candidate sources:
  a hardcoded engine sea-level constant, or the NAM2 `water_form`→WATR record
  (but WATR defines water *type*, not height). This needs research/spec
  verification — guessing a default (e.g. 0.0) violates the no-guessing policy.
- A real correctness sub-bug exists independently: cells with
  `XCLW == -2147483648.0` (the "no water" sentinel, 170 cells) would currently
  spawn a water plane at -2.1e9 (`exterior.rs:262` treats any `Some(h)` as
  "spawn"). The sentinel should suppress the plane.

## Why this is a STOP-and-confirm
1. The audit's specific fix (DNAM) is data-verified inapplicable to Oblivion.
2. The correct fix needs the Oblivion default-water-height *source*, which is
   unverified (no-guessing forbids inventing it).
3. It's a >5-file feature (WorldspaceRecord + wrld.rs + cell water collapse +
   build_exterior_world_context + load_one_exterior_cell + tests) → Phase-4 gate.

Not implemented; surfaced to user with options.
