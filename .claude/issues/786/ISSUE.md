# R-N2 / #786 — perturbNormal disabled by default

**Severity**: HIGH
**Domain**: renderer (Shader Correctness)
**Status**: NEW (workaround landed in `77aa2de`; this is the follow-up tracking issue)

## Locations
- `crates/renderer/shaders/triangle.frag:853-858`
- `crates/renderer/shaders/triangle.frag:646-672`

## One-line summary
Per-fragment normal-map perturbation is gated off by default pending RenderDoc-grade diagnosis of the chrome-on-walls regression. Default render path forfeits all bump detail.

## Fix path
1. `BYROREDUX_RENDER_DEBUG=0x28` capture at chrome-affected camera angle (FNV `GSDocMitchellHouse` recommended per `77aa2de` body)
2. If Path 1 fires green + chrome → fix bitangent sign / tangent handedness
3. If Path 2 fires red + chrome → fix screen-space derivative TBN sign

## Audit source
`docs/audits/AUDIT_RENDERER_2026-05-03.md` finding R-N2.
