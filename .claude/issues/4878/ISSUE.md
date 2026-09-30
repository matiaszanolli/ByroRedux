# #4878: REN-D11-2026-09-24-07: FSR plan §1.4 says 'Auto exposure is not enabled' and omits the exposure meter and AgX

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D11-2026-09-24-07**._

**Severity**: LOW · **Status**: NEW · every claim below was checked against the code by the auditor and re-checked against the tip of `main` at publish time.

**Dimension**: FSR/Presentation

`fsr3-upscaler-integration-plan.md` §1.4 "Exposure" still says "Auto exposure is not enabled" and does not mention the meter, AgX, per-FIF slot ownership or the SRO layout contract; extend `fsr_plan_attributes_exposure_to_the_shader_that_applies_it` to reject that string.

## Completeness Checks
- [ ] **SIBLING**: The same stale claim is checked in the neighbouring docs and code comments (doc moves with the pass)

