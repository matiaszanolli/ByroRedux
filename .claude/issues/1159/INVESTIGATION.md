# #1159 Investigation

## Verification

The bug was real and current at HEAD `03b5b631`:

```glsl
// crates/renderer/shaders/svgf_temporal.comp:215-221 (pre-fix)
uint nearID = texelFetch(prevMeshIdTex, q, 0).r;
// ...
vec3 nearN = octDecode(texelFetch(prevNormalTex, q, 0).rg);
if (nearID == currID && dot(currN, nearN) >= 0.9) {  // ← unmasked equality
```

Compare with the bilinear loop's masked equality at line 156:
```glsl
if ((prevID & 0x7FFFFFFFu) != (currID & 0x7FFFFFFFu)) continue;
```

The unmasked comparison rejects sub-pixel-motion fallback taps whenever the prior frame stamped bit 31 (`ALPHA_BLEND_NO_HISTORY`, per #904 / #992) — even when the underlying 31-bit instance ID matches.

## Fix

One-line predicate update at line 221 (now 229 after the comment block landed):

```glsl
if ((nearID & 0x7FFFFFFFu) == (currID & 0x7FFFFFFFu) && dot(currN, nearN) >= 0.9) {
```

Plus a 16-line comment block above the predicate explicitly linking back to the bilinear loop's mask (the "co-maintenance hint" the issue asked for under TESTS). The comment notes that masking `currID` is a no-op in practice (early-out near line 97 guarantees currID's bit 31 is unset) — the actual fix is masking `nearID`. Kept the `currID` mask for visible symmetry so the two predicates stay co-maintainable.

## SIBLING check

Per the issue: "Cross-check sibling mesh-ID comparisons in the same file (already audited — only these two sites)". Confirmed by grepping the file:
- Line 156: bilinear loop predicate (masked, correct)
- Line 229: sub-pixel-motion fallback predicate (now masked after this fix)

No other mesh-ID equality comparisons in `svgf_temporal.comp`.

## Verification

- `cargo check -p byroredux-renderer`: clean
- `cargo test -p byroredux-renderer --lib`: 278/278 pass
- `glslangValidator -V svgf_temporal.comp -o svgf_temporal.comp.spv`: clean (no errors, no warnings)
- SPIR-V committed alongside the GLSL source

## TESTS gap (acknowledged)

Issue explicitly notes: "Hard to unit-test in isolation; rely on visual A/B at the next prospector/Markarth probe." Closing that gap requires a RenderDoc-driven harness which is the same blocker as TD9-200/201 (today's tech-debt audit) and #1231 (today's Dim 14 audit). Co-maintenance comment block is the next-best regression guard — a future refactor that touches one predicate without the other will surface the asymmetry visually.
