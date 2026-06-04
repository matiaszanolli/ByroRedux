# Investigation — #1462 volumetric depth-convention mismatch

**Domain:** renderer (volumetrics shaders)

## Decision
LOW / latent. The output is gated off (`VOLUMETRIC_OUTPUT_CONSUMED == false`),
so the inject+integrate dispatch is skipped and the composite read is dead.
Reconciling the inject-center / integrate-front-slab / composite-edge math now
would be a speculative shader change with failure modes invisible to cargo test
and unobservable in RenderDoc (nothing is dispatched). Per the project
no-speculative-shader-fix policy, the correct action now is the documentation
alternative from the issue ("document the half-slab bias ... reconcile when the
const flips"), placing the note where a future contributor will see it.

## Fix (documentation only — zero behavioural change)
- `volumetrics.rs` const `VOLUMETRIC_OUTPUT_CONSUMED`: added a FLIP CHECKLIST
  enumerating #1462 (depth conventions) and #1463 (per-FIF UBO) as prerequisites
  to flipping the const.
- `volumetrics_inject.comp:104`, `volumetrics_integrate.comp:64`,
  `composite.frag:396`: one cross-reference comment each, naming the three
  conventions (center / front-of-slab / texel-edge) and the ~half-slab bias, so
  the three sites self-document and point back to the FLIP CHECKLIST.

Shaders recompiled; the `.spv` are byte-identical (comments don't affect SPIR-V),
confirming zero behavioural change.

## Completeness checks
- [x] **SIBLING**: all three depth-convention sites annotated in lockstep.
- [x] **TESTS**: N/A — documentation only; no behaviour to regress. cargo test 2790 pass.
- [x] **No-speculative-fix**: math left untouched while disabled/untestable; the
  reconciliation is captured at the flip site for when it becomes observable.

## Residual
The actual reconciliation is deferred to the M-LIGHT v2 changeset that flips the
const, now gated by the in-code FLIP CHECKLIST.
