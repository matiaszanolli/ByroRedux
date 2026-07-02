# 1823: FO4-D2-01: Regression of #1651 — BGSM/BGEM blend-factor 0↔1 swap corrupts the real authored Additive + Multiplicative modes

URL: https://github.com/matiaszanolli/ByroRedux/issues/1823
Labels: bug, renderer, high, legacy-compat

**Severity**: HIGH
**Dimension**: 2 — BGSM/BGEM Consumption
**Location**: `byroredux/src/asset_provider/material.rs:497-503` (`gl_to_gamebryo_blend`), applied at `:936-937` (BGSM) and `:1032-1033` (BGEM)
**Status**: NEW — **regression of the *intent* of #1651** (commit `ada75ee3`, closed 2026-06-19)
**Regression note**: `#1651` shipped a fix for a real bug (additive BGEM cards rendering invisible) but the fix itself encodes a false premise and introduces a new, worse bug. This is not "still broken" — it is a wrong fix that corrupts two of the three real blend modes while masking the corruption behind the one mode (Standard) that happens to be a fixed point of the swap.

## Description

`#1651` added `gl_to_gamebryo_blend`, which swaps `0↔1` on the BGSM/BGEM
`alpha_blend_mode.src_blend`/`dst_blend` before writing
`mesh.src_blend_mode`/`dst_blend_mode`, on the premise that the file stores a
"GL-style enum (Zero=0, One=1)" that is inverted from the renderer's Gamebryo
nibble. **That premise is false.**

I independently re-derived this from the primary sources (not just the audit's
narrative):

1. **Reference parser** — `/mnt/data/src/reference/Material-Editor/MaterialLib/BaseMaterialFile.cs:363-387` (`ConvertAlphaBlendMode`), read directly. The only `(function=a, src=b, dst=c)` tuples Bethesda's own tooling recognizes as authored are:
   - `None` = `(0, 0, 0)`
   - `Standard` = `(1, 6, 7)`
   - `Additive` = `(1, 6, 0)`
   - `Multiplicative` = `(1, 4, 1)`

   Note `function` is **only ever 0 or 1** in this table — it never takes 2 or 3, which contradicts the doc comment in `crates/bgsm/src/base.rs:45` ("function: 0 = None, 1 = Standard, 2 = Additive, 3 = Multiplicative"). The mode is distinguished by the `(src, dst)` pair, not by `function`.

2. **Renderer table** — `crates/renderer/src/vulkan/pipeline.rs:162-177` (`gamebryo_to_vk_blend_factor`), read directly: `0=ONE, 1=ZERO, 4=DST_COLOR, 6=SRC_ALPHA, 7=ONE_MINUS_SRC_ALPHA`.

3. Feeding the reference's raw stored values straight through the renderer's table (i.e. **no** swap) already gives correct blending for all three real modes:
   - Standard `(6,7)` → `SRC_ALPHA, ONE_MINUS_SRC_ALPHA` — correct standard alpha blend.
   - Additive `(6,0)` → `SRC_ALPHA, ONE` — correct additive accumulation.
   - Multiplicative `(4,1)` → `DST_COLOR, ZERO` — correct multiply (`dst *= src`).

4. The pre-`#1651` code comment ("Blend-factor enums align 1:1 with the Gamebryo AlphaFunction byte the renderer already speaks") was correct. `#1651`'s premise ("GL-style enum") does not match anything in the reference parser — real GL blend enums are large hex constants (`GL_SRC_ALPHA = 0x0302`), not small integers like `6`.

5. Applying the `0↔1` swap corrupts the two modes that touch `0`/`1`:
   - Additive dst `0` → swapped to `1` → `gamebryo_to_vk_blend_factor(1) = ZERO` → result is `SRC_ALPHA·src + ZERO·dst` = **alpha-weighted opaque overwrite, not additive**.
   - Multiplicative dst `1` → swapped to `0` → `gamebryo_to_vk_blend_factor(0) = ONE` → result is `DST_COLOR·src + ONE·dst` = **destination leaks through, wrong multiply**.
   - Standard `(6,7)` is a fixed point of the swap (neither 6 nor 7 is touched), so glass/decal content is unaffected — this is exactly why the regression slipped through review and CI.

6. The `#1651` commit's own additive test case uses a synthetic `(function=2, src=1, dst=1)` — a tuple that **does not appear** in the reference parser's tuple table at all (`function` is never `2`, and `(1,1)` is not a value pair `ConvertAlphaBlendMode` ever emits for a real mode). The test green-lights the wrong translation because it pins a fictional input.

## Evidence

- Reference tuples (verified in-repo): `/mnt/data/src/reference/Material-Editor/MaterialLib/BaseMaterialFile.cs:363-387`.
- Current swap: `gl_to_gamebryo_blend` — `byroredux/src/asset_provider/material.rs:497-503` (`0 => 1`, `1 => 0`, `other => other as u8`).
- Renderer table: `crates/renderer/src/vulkan/pipeline.rs:162-177`.
- Application sites: BGSM branch `byroredux/src/asset_provider/material.rs:936-937`; BGEM branch `:1032-1033`.
- `#1651` diff (`git show ada75ee3`): replaces a correct verbatim-forward comment with the swap, and rewrites the additive test to pin `(1,1)` — a value not authored by any real Bethesda material per the reference tuple table.
- Existing test using the synthetic tuple: `byroredux/src/asset_provider/tests.rs` (`bgsm_merge_forwards_alpha_blend_mode`, additive case, `function=2, src=1, dst=1`).

## Impact

FO4 additive-glow / energy-effect BGEM+BGSM cards (muzzle flashes, energy-weapon beams, glow decals, force-field overlays) render as alpha-weighted opaque instead of additive; multiplicative cards render with a wrong destination term (dest leaks through). Wrong `Material`/blend state out of the parser→renderer boundary → all-FO4 blast radius (also live on FO4 precombine static meshes since `efd3c41b`, and cross-game on FO76/Starfield paths that share this merge code). Blend state is invisible to `cargo test` — this needs a RenderDoc capture to catch, which is exactly how it slipped through `#1651`'s own review.

## Suggested Fix

Re-derive the mapping from the four authoritative `ConvertAlphaBlendMode` tuples end-to-end (author intent → `vk::BlendFactor`) rather than the unverified "GL enum" premise:
1. Verify each of Standard `(6,7)`, Additive `(6,0)`, Multiplicative `(4,1)`, None `(0,0,0)` produces its correct `vk` factor pair with **no** `0↔1` swap (i.e. the pre-`#1651` verbatim forward was correct).
2. Remove/replace `gl_to_gamebryo_blend`'s swap (or make it identity, pending fix design) at both the BGSM and BGEM call sites.
3. Replace the synthetic `(function=2, src=1, dst=1)` test fixture with the real Additive `(function=1, src=6, dst=0)` and Multiplicative `(function=1, src=4, dst=1)` fixtures pinned against the reference tuple table.
4. Confirm with a RenderDoc capture of a real FO4 additive card (muzzle flash / glow decal) before committing, since this exact class of bug is invisible to `cargo test`.

## Completeness Checks
- [ ] **SIBLING**: Both the BGSM (`:936-937`) and BGEM (`:1032-1033`) application sites are fixed together — they share `gl_to_gamebryo_blend`.
- [ ] **CANONICAL-BOUNDARY**: The fix stays at the parser→`Material` merge boundary (`merge_bgsm_into_mesh` in `byroredux/src/asset_provider/material.rs`) — never pushed into the renderer or re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: Replace the synthetic `(2,1,1)` fixture with real Additive `(1,6,0)` / Multiplicative `(1,4,1)` / Standard `(1,6,7)` tuples from the reference parser; assert the resulting `vk::BlendFactor` pairs via `gamebryo_to_vk_blend_factor`, not just the intermediate Gamebryo byte.

