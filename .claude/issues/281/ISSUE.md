# #281: C2-01 — Missing HOST->FRAGMENT_SHADER barrier for composite parameter UBO

**Severity**: MEDIUM | **Domain**: renderer | **Type**: bug

## Finding
`composite.upload_params()` host-writes the per-frame UBO (`draw.rs:598`) with no
memory barrier before `composite.dispatch()` (`draw.rs:609`). SVGF and SSAO both
emit HOST→COMPUTE barriers for their UBOs; composite omits this.

## Fix
Add `cmd_pipeline_barrier` with HOST→FRAGMENT_SHADER and HOST_WRITE→UNIFORM_READ.
