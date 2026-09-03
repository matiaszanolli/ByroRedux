# #3742 — TD2-2026-08-30-02: the 64-entry BLUE_NOISE_RANKS table is byte-identical in two shaders that already share the include it belongs in

**Severity**: LOW · **Location**: `crates/renderer/shaders/composite.frag`, `crates/renderer/shaders/volumetrics_inject.comp`
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-30.md` (TD2-2026-08-30-02)

Both shaders declared a byte-identical 64-entry `BLUE_NOISE_RANKS` table (verified diff-clean
before touching anything), each with its own consuming function (`preResolveDither` /
`blueNoiseRank`). Both already `#include "include/shader_constants.glsl"`.

## Fix implemented

- New `crates/renderer/shaders/include/blue_noise.glsl` holds the one copy of the table.
- Both `composite.frag` and `volumetrics_inject.comp` now `#include "include/blue_noise.glsl"`
  and no longer declare their own copy. `volumetrics_inject.comp`'s explanatory comment (why a
  compact constant beats a texture fetch here) moved to sit above its own consumer,
  `blueNoiseRank`, since the table declaration itself moved out.
- `shader_constants_data.rs` (the documented single source for `#define`-style scalar shader
  constants) was **not** used — that generation pipeline only emits scalars, and extending it to
  emit array literals would be a bigger, riskier change than a dedicated GLSL include for one
  64-entry array. The issue's own suggested fix offered this as an equally valid alternative.
- Recompiled both `.spv` files (`glslangValidator -V -I.`). The resulting bytecode is
  **byte-identical** to what was already committed (`git status`/`git diff` show no change to
  either `.spv`) — strong confirmation this is a pure source-level dedup with zero behavior
  change at the GPU level.

Regression test (issue's own TESTS checklist item):
`blue_noise_ranks_is_declared_exactly_once` (`crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs`)
asserts the table is declared in `include/blue_noise.glsl` and that neither `composite.frag` nor
`volumetrics_inject.comp` re-declares it or omits the `#include`.

**SIBLING** (issue's own checklist item): grepped every GLSL array-constant declaration
(`= uint[`/`= float[`/`= vecN[`/`= int[`) across `crates/renderer/shaders/*.{frag,vert,comp}` —
found in `svgf_atrous.comp`, `svgf_temporal.comp`, `triangle.frag`, and two sites within
`volumetrics_inject.comp` itself. None share a name or content across files (a 3-tap Gaussian
kernel, a bilinear weight array, per-material decal indices, offset arrays) — no other
cross-shader duplicate table found.

Full workspace: `cargo test --no-fail-fast` 7046 passing, 0 failing.

## Note: cross-session collision during verification

While attempting a stash-based pre-fix verification, a `git stash push` with an invalid pathspec
(an untracked file, without `-u`) failed silently and left the working tree unchanged; the
follow-up `git stash pop` then popped an **unrelated, pre-existing stash from a concurrent
session** (`stash@{0}: WIP on main: e309cc5 ...`), producing a merge conflict in
`.claude/settings.json`. Resolved by keeping the committed HEAD content for that file
(`git checkout --ours` + `git add`) — verified it now matches HEAD exactly, so nothing from this
session lands in it. The peer session's stash itself was **not** dropped by the conflicted pop
(git's own safety behavior) and remains in the stash list, recoverable by that session
whenever needed. No source files were affected; this fix's own four files were untouched
throughout.
