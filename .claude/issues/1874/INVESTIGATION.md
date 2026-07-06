# #1874 — DIAGNOSTIC ADDED (not fixed), 2026-07-05

## Why this isn't a code fix
The ghosting is a tracking issue whose own body proposes no fix and calls for a
RenderDoc capture. A 4-dimension audit (AUDIT_RENDERER_2026-07-04 Symptom 2)
narrowed the mechanism (H1: a spatially-uniform bad motion vector shared by
SVGF + TAA, frozen by TAA's intentional #1479 parked-camera clamp bypass) but
could not find the origin — every motion-vector authoring site reads correct on
static analysis. A speculative Vulkan/shader behaviour change would be invisible
to `cargo test` and risk regressing #1479 — against the no-speculative-fix policy.

## Independent re-trace this pass — two more uniform-offset suspects cleared
1. **Origin-rebase correction** (`origin_corrected_prev_view_proj`, draw.rs:3960)
   — sound (`M·(x−O₂) = prev_vp·(x−O₁)`) and unit-tested at Markarth scale.
2. **Jitter mismatch** — triangle.vert:175-177 keeps `fragCurrClipPos`
   un-jittered (jitter applied to `gl_Position` only, after capture, line 256);
   `fragPrevClipPos` uses the un-jittered `prevViewProj`. Motion correctly
   excludes TAA jitter.
Both prime sources of a *uniform depth-independent* offset are clean —
reinforcing that the fault (if H1) is a transient wrong `prevViewProj` value on
a specific frame event, observable only live.

## Added: DBG_VIZ_MOTION (0x20000) — instrumentation, not behaviour change
Renders `outMotion` (the velocity SVGF + TAA reproject with) as colour
(`rg = 0.5 + motion.xy*64`, `b = 0.5`), gated entirely behind the debug bit.
Answers the audit's decisive checklist item #1 live, without RenderDoc:
- flat grey → no motion (healthy static camera);
- **uniform full-screen tint → spatially-uniform bad motion vector** (H1: camera-level prevViewProj/origin);
- depth-varying tint → real parallax (correct);
- tint localised to a body → skinning.
Usage: reproduce ghost, park camera, `BYROREDUX_RENDER_DEBUG=0x20000`.

Wiring (shader-sync lockstep): `shader_constants_data.rs` const (source of
truth) → `build.rs` emits GLSL `#define` → regenerated `shader_constants.glsl`
→ `triangle.frag` branch → recompiled `triangle.frag.spv` (plain `-V`).

## Status
LEFT OPEN — the fix depends on what the viz reveals on an affected frame.
Full workspace green; renderer suite green (362, incl. SPV reflection test).
