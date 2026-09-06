# #3881: TD7-2026-09-05-03: The mesh-ID no-history bit and its complement mask are hand-typed at five GLSL sites — including the shader that *writes* the bit — and have no Rust-side constant at all

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD7-2026-09-05-03) via `/audit-publish`, 2026-09-05. Labels: `low,shaders,renderer,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3881 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD7-2026-09-05-03), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 7 — Magic Numbers & Hardcoded Constants
- **Location**: `crates/renderer/shaders/include/mesh_id.glsl` (`MESH_ID_NO_HISTORY_BIT`) · `crates/renderer/shaders/triangle.frag` (the `outMeshID` write and the two `sortedInstanceId` / `stableSurfaceId` masks) · `crates/renderer/shaders/caustic_splat.comp` (the `meshIdRaw` test and mask)
- **Status**: NEW
- **Effort**: trivial (≤30 min)
- **Description**: bit 31 of the `R32_UINT` mesh-ID G-buffer attachment is the `ALPHA_BLEND_NO_HISTORY` flag: set, it switches the low 31 bits into the alpha draw-index namespace and tells TAA/SVGF the pixel has no stable temporal history. It has exactly one named declaration — `const uint MESH_ID_NO_HISTORY_BIT = 0x80000000u;` in `include/mesh_id.glsl` — and that header is `#include`d by only three of its five consumers (`taa.comp`, `svgf_atrous.comp`, `svgf_temporal.comp`). The two that do not include it are the **producer** and the caustic reader, both of which hand-type the raw literal.
- **Evidence**:
  - `include/mesh_id.glsl` declares the bit and the `meshIdHasStableHistory` / `stableMeshIdsMatch` helpers. `grep -rn 'mesh_id.glsl'` over the shader tree returns exactly three `#include` lines: `taa.comp:6`, `svgf_atrous.comp:6`, `svgf_temporal.comp:6`.
  - `triangle.frag` — the writer — emits `outMeshID = meshIdBase | (alphaBlendFrag ? 0x80000000u : 0u);` and masks the two ID lanes with `& 0x7FFFFFFFu` twice, immediately above it. No include.
  - `caustic_splat.comp` reads `if ((meshIdRaw & 0x80000000u) == 0u) return;` then `uint meshId = meshIdRaw & 0x7FFFFFFFu;`. No include.
  - **Nothing on the Rust side declares it.** `scene_buffer/constants.rs`, `vulkan/context/helpers.rs` and `vulkan/pipeline.rs` each describe `0x80000000` in a *comment* — three separate prose restatements of a bit with no code-level definition on their side of the boundary. (`shader_constants_data.rs`'s `NORMAL_ALPHA_SPEC_BIT` / `PARALLAX_ALPHA_HEIGHT_BIT` / `DBG_VIZ_SELECTED_LIGHT` all share the numeric value `0x80000000` for unrelated fields, so a value-based search cannot disambiguate this one either.)
  - The complement `0x7FFFFFFFu` is written three times and never expressed as `~MESH_ID_NO_HISTORY_BIT`.
- **Impact**: the producer and one reader can drift from the header independently, and a search for the bit's definition from the Rust side finds only comments. This is the identical shape as #2265 (one 8-layer budget, three independent GLSL declarations), #2045 (`INST_RENDER_LAYER_SHIFT`/`_MASK` hand-written in `triangle.frag`) and #3745 (RT reach budgets hand-typed at six sites) — all closed; this instance was missed because the declaration lives in `include/`, which the provenance gate does not scan (TD7-2026-09-05-02). Low severity: the bit is correctly typed at every site today, so nothing is currently wrong.
- **Related**: #2265 / TD7-001 · #2045 / TD7-101 · #3745 / TD7-2026-08-30-01 · #1780 / D14-LOW-01 · TD7-2026-09-05-02
- **Suggested Fix**: move the bit into `crates/renderer/src/shader_constants_data.rs` (it is a genuine cross-CPU/GPU attachment ABI, and the Rust-side comments in `scene_buffer/constants.rs` and `context/helpers.rs` already want to reference it by name), emit a companion `MESH_ID_STABLE_MASK` `#define`, and have `include/mesh_id.glsl` consume the generated header rather than redeclaring. Then `#include` it from `triangle.frag` and `caustic_splat.comp` and replace all five literals.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
