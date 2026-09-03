# #3745 — TD7-2026-08-30-01: water.frag's three RT reach budgets bypass shader_constants_data.rs

**Severity**: MEDIUM · **Location**: `crates/renderer/shaders/water.frag:194-196`, `crates/renderer/src/shader_constants_data.rs`
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-30.md` (TD7-2026-08-30-01)

`water.frag` declared `REFLECTION_MAX_DIST`/`REFRACTION_MAX_DIST`/
`DIST_FALLOFF` as local `const float`s, none present in
`shader_constants_data.rs` (the documented single source both
`shader_constants.rs` and `build.rs` `#include!`). The two `MAX_DIST`
values mirrored `triangle.frag` by hand and by literal across six sites in
two shaders with no shared definition. `DIST_FALLOFF`'s trailing `// matches
triangle.frag` comment was false: `0.0015` appeared nowhere in
`triangle.frag`, which has no `DIST_FALLOFF` at all — its nearest analogue
(`0.004`, glass optical thickness) is a different quantity.

## Fix implemented (steps 1-2 of the issue's own three-step suggested fix)

- Added `RT_REFLECTION_MAX_DIST` (5000.0), `RT_REFRACTION_MAX_DIST` (2000.0),
  `RT_DIST_FALLOFF` (0.0015) to `shader_constants_data.rs`, with a doc
  comment correcting the false "matches triangle.frag" claim for
  `RT_DIST_FALLOFF` specifically (it has no current `triangle.frag`
  consumer — centralized anyway per the file's single-source-of-truth
  doctrine).
- `build.rs`: emits the three as `#define`s into the generated
  `shader_constants.glsl`.
- `water.frag`: removed the three local `const float` declarations and the
  false comment; its three use sites now reference the shared `#define`s
  (both files already `#include "include/shader_constants.glsl"`).
- `triangle.frag`: all four reach-budget sites (`traceReflection`'s window-
  distortion ray, the window "outside" ray query, `REFRACT_MAX_REACH`'s own
  declaration — kept as a local semantic alias initialized from the shared
  constant rather than removed outright, since it's used at multiple
  call sites within its own scope — and the general reflection ray) now
  reference `RT_REFLECTION_MAX_DIST`/`RT_REFRACTION_MAX_DIST` instead of
  hardcoded `5000.0`/`2000.0`.

**Verified zero behavior change**: recompiled both `water.frag.spv` and
`triangle.frag.spv` (`glslangValidator -V -I.`) and diffed against the
pre-fix committed bytecode — **byte-for-byte identical** in both cases.
Since the numeric values are unchanged, only their declaration site moved,
this is the strongest available confirmation that the GPU executes exactly
the same instructions as before.

One additional site was found during investigation but is **out of the
issue's own stated scope** ("six sites for two ray-reach budgets") and left
untouched: `water.frag`'s caustic floor-finding ray
(`rayQueryInitializeEXT(..., 5000.0)`, a "find the floor" query, not the
reflection/refraction visual budget the issue is about) — noted here for a
future pass rather than silently expanded into.

**SIBLING** (issue's own checklist item): a full sweep for every top-level
shader `const` name absent from `shader_constants_data.rs` is exactly step
3 of the issue's own suggested fix — see below.

## Part 3 (deliberately NOT implemented here, filed separately as #3815)

Strengthening the gate from a per-name allowlist to a structural check —
the issue's own words: "**Without (3) this dimension will keep re-finding
new instances.**" — is real new test infrastructure (scanning every shader
file for undeclared top-level consts, cross-referencing against
`shader_constants_data.rs`, and designing a correct exemption list for
genuinely shader-local values) with real design tradeoffs, not a mechanical
extension of the existing per-name assertions. Filed as #3815 rather than
rushed here.

**TESTS** (issue's own checklist item — "fix (3) *is* the test", which is
exactly why it's deferred to #3815): added
`water_frag_rt_reach_budgets_not_redeclared` (mirrors the existing
`water_frag_motion_enum_matches` redeclaration-guard shape: asserts
`water.frag` doesn't redeclare any of the three names, and no longer
contains the false "matches triangle.frag" claim) and extended
`generated_header_contains_all_defines` with the three new `#define`
value-pins.

Full workspace: `cargo test --no-fail-fast` 7063 passing, 0 failing (+1 new
test).
