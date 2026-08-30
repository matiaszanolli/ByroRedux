# #3745 — TD7-2026-08-30-01: `water.frag`'s three RT reach budgets bypass `shader_constants_data.rs`, and its `// matches triangle.frag` claim is false

**Labels**: bug, renderer, medium, tech-debt, shaders

---

- **Severity**: MEDIUM
- **Dimension**: 7 — Magic Numbers / Shader-Constant Provenance
- **Location**: `crates/renderer/shaders/water.frag:194-196`; the gate at `crates/renderer/src/shader_constants.rs`; the single source `crates/renderer/src/shader_constants_data.rs`
- **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-30.md` (`TD7-2026-08-30-01`), HEAD `64f64480`

## Description

```glsl
const float REFLECTION_MAX_DIST = 5000.0;
const float REFRACTION_MAX_DIST = 2000.0;
const float DIST_FALLOFF        = 0.0015; // matches triangle.frag
```

**None of the three exists in `crates/renderer/src/shader_constants_data.rs`** (verified
at HEAD: 0 hits) — the documented single source that `shader_constants.rs` and `build.rs`
both `include!` to emit `shaders/include/shader_constants.glsl`. They are open-coded
literals in a shader that *does* `#include` that header for its other constants.

**The `// matches triangle.frag` comment is not true.** `0.0015` appears exactly once in
the entire shader tree — on this line. `triangle.frag` has no `DIST_FALLOFF` and no
`0.0015`; its nearest analogue is the glass optical-thickness `0.004`, a different
quantity. So the comment asserts a lockstep relationship that (a) is unenforced and
(b) does not currently hold.

The two `MAX_DIST` values *do* mirror `triangle.frag`, but by hand and by literal:
`5000.0` at `triangle.frag:2641`, and `2000.0` at `triangle.frag:1041`, `:1652` and
`:1966` (`const float REFRACT_MAX_REACH = 2000.0;`). `triangle.frag:1954` is candid about
it — *"re-issued the query with a fresh **hard-coded** 2000.0 tMax"*. That is six sites
for two ray-reach budgets across two shaders with no shared definition.

## Why the existing gate does not catch this

`crates/renderer/src/shader_constants.rs` enforces provenance with a **per-shader named
allowlist**, not a structural rule: it asserts `!src.contains("const uint WATER_CALM")`,
`!src.contains("const float BLOOM_INTENSITY")`, `!src.contains("const float VOLUME_FAR")`,
`!src.contains("const uint THREADS_PER_CLUSTER")` and so on — each name someone remembered
to list. `water_frag_motion_enum_matches` guards five `WATER_*` names **in this very file**
and walks straight past the three constants three lines above them. **The gate can only
catch redeclarations of enumerated names, so any newly introduced literal is invisible to
it by construction.**

## Suggested Fix (in order)

1. Move all three into `shader_constants_data.rs` so both shaders `#include` one
   definition — this makes the `// matches triangle.frag` intent real instead of
   aspirational. *(small)*
2. Delete the now-redundant comment. *(trivial)*
3. **Strengthen the gate from a name allowlist toward a structural check** — e.g. assert
   that no `crates/renderer/shaders/*.{frag,vert,comp}` declares a top-level
   `const float|uint|int <SCREAMING_NAME>` unless that name is present in
   `shader_constants_data.rs`, with a small explicit exemption list for genuinely
   shader-local values. **Without (3) this dimension will keep re-finding new instances.**
   *(medium)*

Severity MEDIUM per the lockstep-drift floor (`feedback_shader_struct_sync.md`); the
`_audit-severity.md` HIGH floor applies to `#[repr(C)]`/struct drift, and these are scalar
budgets.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — sweep every shader for top-level `const` names absent from `shader_constants_data.rs`, not just `water.frag`
- [ ] **TESTS**: A regression test pins this specific fix (fix (3) *is* the test)
