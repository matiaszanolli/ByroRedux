# #3607 — REN-2026-08-30-D13-04: `taa.comp` holds a fourth, differently-named copy of the octahedral decoder that the shared-copy maintenance comment does not enumerate

**Labels**: `low,renderer,shaders,tech-debt,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3607 --json state`.

---

- **Severity**: LOW
- **Dimension**: TAA
- **Location**: `crates/renderer/shaders/taa.comp` (`oct_decode`, line 36); `crates/renderer/shaders/svgf_atrous.comp` (lines 77–80, the enumeration); siblings `crates/renderer/shaders/svgf_temporal.comp:68`, `crates/renderer/shaders/caustic_splat.comp:176`; encoder at `crates/renderer/shaders/include/math_common.glsl:35`
- **Status**: OPEN — duplication + stale maintenance note
- **Description**: The octahedral **encoder** is centralised (`octEncode` in `include/math_common.glsl`, the single function every `outNormal` write in `triangle.frag` goes through). The **decoder** is not: it is copy-pasted into four shaders. Three of them spell it `octDecode` and carry a maintenance comment enumerating the other copies — `svgf_atrous.comp:79` says it "must stay bit-identical to the `octDecode` copies in `svgf_temporal.comp` and `caustic_splat.comp`". `taa.comp` spells its copy `oct_decode` (snake_case, unlike every sibling) and is absent from that enumeration, so neither a `grep octDecode` nor the comment leads a maintainer to it.
- **Evidence**:
  - `grep -rn "oct_decode\|octDecode" crates/renderer/shaders/` → `taa.comp:36,175,176` (`oct_decode`) plus `svgf_atrous.comp:80`, `svgf_temporal.comp:68`, `caustic_splat.comp:176` (`octDecode`). `include/math_common.glsl` defines `octEncode` only.
  - `svgf_atrous.comp:77–79` names exactly two sibling copies; taa.comp is the third sibling and is not named.
  - The four bodies are currently identical in behaviour (`n.z = 1 - |x| - |y|`, wrap-fold when `n.z < 0`, `normalize`), so this is drift *risk*, not present drift.
- **Impact**: `taa.comp`'s only use of the decoder is the surface-consistency disocclusion test (`dot(currNormal, prevNormal) < 0.85`, `taa.comp:175–177`) that the `taa_comp_keeps_history_bounded_and_rejects_unstable_surfaces` guard depends on. A future correction to the shared decode (precision, a `normalize` removal, a snorm-range change) applied to the three enumerated copies would leave TAA decoding differently from the G-buffer producer and from SVGF — a silent, per-pixel divergence in a history-rejection predicate, invisible to every existing test since all of them are source-scan pins.
- **Suggested Fix**: Move the decoder next to `octEncode` in `include/math_common.glsl` as `octDecode`, have all four shaders `#include` it (`taa.comp` already uses `GL_GOOGLE_include_directive` for `shader_constants.glsl` and `mesh_id.glsl`), and delete the enumeration comments that only existed to compensate for the duplication. Minimum change if the include is deferred: rename `taa.comp`'s copy to `octDecode` and add it to the `svgf_atrous.comp` enumeration so the existing convention at least finds it.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D13-04

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
