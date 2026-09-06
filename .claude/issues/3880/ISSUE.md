# #3880: TD7-2026-09-05-02: #3815's shader-constant provenance gate is blind to function-local `const`s, every `#define`, and the whole `shaders/include/` tree

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD7-2026-09-05-02) via `/audit-publish`, 2026-09-05. Labels: `low,shaders,renderer,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3880 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD7-2026-09-05-02), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 7 — Magic Numbers & Hardcoded Constants
- **Location**: `crates/renderer/src/shader_constants.rs::top_level_shader_constants`, `::renderer_shader_sources`, `::every_top_level_shader_constant_has_one_provenance`
- **Status**: NEW (Regression-adjacent to #3815, CLOSED 2026-09-03 — the gate landed, its coverage claim is broader than its implementation)
- **Effort**: medium (≤1 day)
- **Description**: #3815 replaced a per-name allowlist with a structural check whose doc comment says it will *"scan the complete shader directory rather than maintaining a growing per-name allowlist."* Three independent narrowings mean the check reaches only a minority of shader constant declarations, and every constant that currently bypasses `shader_constants_data.rs` sits in one of the blind spots.
- **Evidence**: read against current code, three separate filters:
  1. **`renderer_shader_sources()` is not recursive and excludes `.glsl`.** It calls `std::fs::read_dir` on `shaders/` once and keeps only `Some("frag") | Some("vert") | Some("comp")`. `shaders/include/` is never opened and `.glsl` is not in the extension list, so **no include file is scanned at all** — including `include/shader_constants.glsl` itself.
  2. **`top_level_shader_constants` requires brace depth 0** on all four tokens of the `const TYPE NAME =` window (`*qualifier_depth == 0 && *ty_depth == 0 && *name_depth == 0 && *equals_depth == 0`). Every function-local `const` is invisible by construction — the doc comment states this as intent, but function-local is where this codebase actually puts its ray/loop budgets.
  3. **Only `const` with type `float | uint | int` is matched** (`matches!(ty.as_str(), "float" | "uint" | "int")`). `#define`, `vec*`, `ivec*`, `bool` and array constants are all unreachable — and `#define` is the form the generated header itself emits, so the "shared-name redeclaration" branch can only ever fire against the `const` spelling.

  What is currently un-gated as a result (all verified present today):
  - `include/pbr.glsl`: `#define SPECULAR_AA_VARIANCE 0.25`, `#define SPECULAR_AA_THRESHOLD 0.2` — both filters 1 and 3.
  - `include/lighting.glsl`: `const int REFLECTION_LIGHT_CANDIDATES = 4;`, `const uint GI_VISIBLE_LIGHT_CAP = 2u;` — the latter is the "first two VISIBLE contributors" that `shader_constants_data.rs`'s `GI_HIT_LIGHT_CAP` doc comment describes in prose, with no mechanical link.
  - `include/shadow_transport.glsl`: `const int MAX_GLASS_INTERFACES = 4;`
  - `include/mesh_id.glsl`: `const uint MESH_ID_NO_HISTORY_BIT = 0x80000000u;` (see TD7-2026-09-05-03).
  - `triangle.frag`: `MAX_SHADOW_RAYS`, `MAX_PATH_SEGMENTS`, `MAX_DIFFUSE_BOUNCES`, `MAX_SHADED_HITS`, `MAX_REFRACT_PASSTHRUS`, `RT_LOD_SCALE`, `RT_LOD_REFLECT`, `RT_LOD_GI`, `AMBIENT_FILL`, `RESERVOIR_W_CLAMP`, `RESTIR_M_CAP`, `SPATIAL_SAMPLES`, `SPATIAL_RADIUS`, `SPATIAL_M_CAP`, `TEMPORAL_NORMAL_COS` — all function-local, all filter 2.

  The gate is not vacuous: `shader_constant_provenance_gate_rejects_synthetic_shared_redeclaration` proves the shared-name branch works, and `SHADER_LOCAL_CONSTANT_EXEMPTIONS` holds 17 entries for the top-level declarations it *does* see (in `ssao.comp`, `svgf_atrous.comp`, `volumetrics_inject.comp`). But the exemption list being 17 long against a reachable population of ~17 means the gate is currently catching zero live violations while ~20 real bypasses sit just outside its reach.

- **Impact**: the codebase believes it has a structural single-source-of-truth guarantee for shader constants (`feedback_shader_struct_sync.md` treats this as the lockstep mechanism) when in practice the guarantee covers only top-level scalar `const`s in `.frag`/`.vert`/`.comp` files. Every finding in this dimension's shader half — TD7-2026-09-05-01 and -03 — is a constant the gate could not see. Low severity because no drift has actually shipped; the debt is a false sense of coverage that will let the next one through.
- **Related**: #3815 (the gate) · #1780 / D14-LOW-01 (`caustic_splat.comp` + `water.frag` missing from an earlier lockstep test — same "the check does not reach the file" shape) · TD7-2026-09-05-01 · TD7-2026-09-05-03
- **Suggested Fix**: walk `shaders/` recursively and add `glsl` to the extension filter; extend the lexer to recognise `#define NAME <literal>` alongside `const`; drop the `brace_depth == 0` requirement (or report function-local declarations under a separate, softer violation class so the ~15 legitimately stage-local ones in `triangle.frag` can be exempted deliberately rather than by accident). Expect the exemption list to grow — that is the point: each entry becomes a recorded decision instead of an invisible bypass.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
