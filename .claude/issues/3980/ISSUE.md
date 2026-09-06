# #3980 — REN-2026-09-06-D10-01: `weatherGroundNoise` hashes ABSOLUTE world XZ through `sin()`, whose argument reaches ~3×10⁷ rad in vanilla exterior worldspaces

**Labels**: medium, renderer, shaders, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D10-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: Camera-Relative Precision
- **Location**: `crates/renderer/shaders/triangle.frag` (`weatherGroundNoise`, and its three call sites in the `terrainSplatActive && jitter.w > 0.5` weather block)
- **Status**: NEW (landed in `01451d5e` "implement weather surface state management for rain and snow effects"; no open issue matches `weather|snow|puddle|noise|hash`)
- **Description**: The weather surface response keys its puddle and snow-patch fields on
  the **absolute** reconstructed position (`fragWorldPos.xz`), not on the render-origin-relative
  varying. Using absolute coordinates here is defensible on its own — a relative key would
  make the pattern jump every time `render_origin` snaps across a 4096-unit cell boundary.
  The problem is the *formulation*: `weatherGroundNoise` is the classic
  `fract(sin(dot(cell, vec2(127.1, 311.7))) * 43758.5453)` sine hash, which multiplies the
  coordinate up by ~440× before the precision-critical `sin`. That amplification is what
  `RT_ABSOLUTE_PRECISION_CEILING` cannot protect against: the ceiling bounds `|coord|` at
  2²⁰, but this consumer fails an order of magnitude *below* it.
- **Evidence**:
  - `float weatherGroundNoise(vec2 worldXZ, float cellSize) { vec2 cell = floor(worldXZ / max(cellSize, 0.001)); return fract(sin(dot(cell, vec2(127.1, 311.7))) * 43758.5453); }`
  - Call sites pass `fragWorldPos.xz` (absolute) with `cellSize` 7.0, 2.5 and 3.5.
  - Worked number at the smallest cell size, on shipped content: `docs/engine/shader-pipeline.md`
    gives Skyrim Tamriel ≈ ±233 000 u and Markarth at X ≈ −176 000. At `cellSize = 2.5`,
    `|cell|` reaches ~93 000; `|dot(cell, vec2(127.1, 311.7))|` reaches ~4.1×10⁷. f32 ULP at
    4×10⁷ (exponent 25) is **4.0 radians** — more than half a full period of `sin`.
  - The gate is `terrainSplatActive`, i.e. this code only ever runs on exterior LAND — the
    exact regime where the argument is largest. It is unreachable in the interiors and
    unit-scale harness scenes where it would be numerically well-behaved.
  - GLSL/SPIR-V places no useful accuracy requirement on `Sin` at arguments this far
    outside `[−π, π]`, so the field is additionally **driver-dependent**: the same cell can
    hash differently on two GPUs.
- **Impact**: Visual only, but real and not reproducible across hardware. The puddle/snow
  patch field stops being the intended per-cell white hash at exterior distances; what it
  actually produces there is unspecified. This also makes any future "does the weather
  system look right?" comparison between two machines meaningless.
- **Related**: `docs/engine/shader-pipeline.md` §"Coordinate Spaces & Precision" ("Any future
  absolute-space shader consumer inherits this same ceiling") — this is the first consumer
  found that needs a *tighter* bound than the ceiling provides. Adjacent but distinct:
  `triangle.frag`'s translucency turbulence proxy `sin(NdotV * 11.0 + fragWorldPos.x * 0.013)`
  scales *down* by 0.013 and stays well-conditioned; not a finding.
- **Suggested Fix**: Replace the sine hash with an integer-domain one that is exact at any
  magnitude — hash the `ivec2` cell index with a bit-mixing function (e.g. Wang/PCG-style
  `uint` mixing on `floatBitsToInt(cell)` or on `ivec2(cell)`), which keeps the absolute
  world key (so the pattern stays anchored across origin snaps) while removing the
  large-argument transcendental entirely. Alternatively keep `sin` but wrap the dot product
  into `[0, 2π)` first — cheaper to write, but it re-quantises rather than fixing the
  precision loss, so the integer hash is the better answer.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
