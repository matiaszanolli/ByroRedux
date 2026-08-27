# FNV-RT-2026-08-26-02

**Issue**: #3322
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: MEDIUM
**Dimension**: 3 — RT Lighting Pipeline
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/renderer/shaders/composite.frag.spv` (stale) vs
`crates/renderer/shaders/composite.frag:505-510`, `include/shader_constants.glsl:196`

**Premise verified**: `ceb69d24` bumped `RENDER_DEBUG_MODE_MAX` 8u → 9u and added
`RENDER_DEBUG_MATERIAL_ROLE 9u` (generated from
`crates/renderer/src/shader_constants_data.rs`; the Rust enum arm exists at
`crates/renderer/src/vulkan/render_debug.rs:26`, `MaterialRole = RENDER_DEBUG_MATERIAL_ROLE`).
`composite.frag.spv` was not recompiled. Disassembly diff of committed vs freshly compiled is
exactly one instruction, nothing else:

```
604c604
<        %1042 = OpUGreaterThan %bool %1041 %uint_8
---
>        %1042 = OpUGreaterThan %bool %1041 %uint_9
```

which is the guard at `composite.frag:505-510`:
```glsl
if (debugMode != RENDER_DEBUG_LEGACY_FLAGS && debugMode > RENDER_DEBUG_MODE_MAX) {
    outColor = vec4(1.0, 0.0, 1.0, 1.0);   // magenta, then return
    return;
}
```

**Impact (FNV-visible)**: `render.debug material_role` — mode 9, the R5.4 role-visualisation
oracle the recovery plan lists as *the* tool for "lobe correct, texture wrong" in its own
triage table — is unusable end-to-end: `triangle.frag` shades it correctly, then composite
discards the frame and paints magenta. Any FNV material-role triage (the "chrome/posterized =
missing textures" workflow this project leans on) silently loses its dedicated view and
falls back to `tex.missing`. Debug-surface only; no gameplay pixel is affected.

**Fix sketch**: same recompile as finding 01 (`composite.frag`); covered by the same
`check-shader-artifacts.sh` green.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
