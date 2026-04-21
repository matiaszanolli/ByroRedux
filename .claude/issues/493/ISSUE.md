# Issue #493: asset_provider BGSM resolver

Severity: HIGH | Labels: bug, renderer, legacy-compat

## Parent: #411. Depends on #BGSM-1 (parser), #BGSM-3 (GpuInstance slots).

## Deliverables
- MaterialProvider in asset_provider.rs opening Materials BA2 + caching parsed+resolved BGSM
- Integration in NIF material flow: merge BGSM into MaterialInfo; NIF fields win
- Fallback on BGSM parse failure: log once, keep NIF defaults

## Deps satisfied
- bgsm crate exists (parse, TemplateCache, TemplateResolver)
- GpuInstance has parallax/env slots (192 bytes, verified in memory)
