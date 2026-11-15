# #6982: TD7-2026-11-15-03: the close-range fire march caps (`160` steps, `64` brick jumps) are derived from `LOCAL_GRID_DIM` / `LOCAL_GRID_BRICK` in prose only (PLAUSIBLE)

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6982
- **Labels**: low,shaders,renderer,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW
- **Dimension**: 7 (magic numbers)
- **Location**: `crates/renderer/shaders/composite.frag:314` and `:322`; exempted at `crates/renderer/src/shader_constants.rs:355,358`; the derivation inputs are `shader_constants_data.rs:2149-2153` and `shaders/include/local_grid_params.glsl:106`
- **Age**: M-LIGHT local-fire work (#6533/#6535), within the window
- **Effort**: trivial
- **Labels**: `tech-debt`
- **Status**: NEW
- **Description**: the comments state the derivations: "A box is 64 cells across, so the cap only matters for a ray running diagonally" (steps) and "at most 3 * 8 bricks are crossed along any axis-aligned path; diagonals cross a few more" (brick jumps). `LOCAL_GRID_DIM = 64` and `LOCAL_GRID_BRICK = 8` are generated defines, and `LOCAL_GRID_BRICKS_PER_AXIS` is already computed in `local_grid_params.glsl:106`, but the two caps are exempted as "stage-local" literals. `shader_constants_data.rs:2165-2167` const-asserts workgroup divisibility only, not the march bound.
- **Evidence**: `const int LOCAL_FIRE_MAX_STEPS_PER_BOX = 160;` and `const int LOCAL_FIRE_MAX_BRICK_JUMPS = 64;`. A diagonal through a 64-cell box is about 111 cells (64 x sqrt 3), so 160 steps leaves about 1.4x slack at the 1-cell step the loop falls back to in busy gas. The same diagonal at `LOCAL_GRID_DIM = 128` is about 222 cells.
- **Impact**: PLAUSIBLE, not reproduced. If `LOCAL_GRID_DIM` is raised the march silently truncates in busy fire and the flame looks clipped, with no test failing. The brick-jump cap holds until about 24 bricks per axis (3 x 24 = 72 > 64). Nothing is wrong at today's 64/8.
- **Related**: #6533, #6535, #6526 (closed, the sibling god-ray bounds); `shader_constants.rs:211` exemption table.
- **Suggested Fix**: derive both from the generated defines (`3 * LOCAL_GRID_BRICKS_PER_AXIS + 8`, and `int(ceil(1.74 * float(LOCAL_GRID_DIM))) + 48` or a named `LOCAL_FIRE_STEP_HEADROOM`), then drop the two exemption rows.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
