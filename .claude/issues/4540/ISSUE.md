# REN-D11-2026-09-20-03: FSR plan §1.4 + exal-groundcover §12.14 say ground cover 'writes zero to both masks' — groundcover_blade.frag has written 0.9 × max(midTransition, cardTransition) reactive since fd0cd577c

- **ID**: REN-D11-2026-09-20-03
- **Labels**: low,renderer,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: FSR/Presentation
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D11-2026-09-20-03)

**Location**: `crates/renderer/shaders/groundcover_blade.frag:247`; docs: `docs/engine/fsr3-upscaler-integration-plan.md` §1.4, `docs/engine/exal-groundcover.md` §12.14

**Description**
The code follows the mask policy (material-driven marking, 0.9 clamp — pinned by `blade_motion_and_fsr_mask_contract_stay_material_driven`); the two doc sites still claim the zero-mask era.

**Evidence**
Audit D11, 2026-09-20.

**Impact**
Mask-contract readers verify against a false premise (same class as the audit-exterior #4506 manual-acceptance note).

**Suggested Fix**
Update both doc sites to cite the transition-driven reactive value and its pin test.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
