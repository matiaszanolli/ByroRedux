# REN-D23-04: BYRO_FSR_FORCE_DISPATCH_FAIL has no cfg gate despite being documented debug-only

Labels: low, renderer, bug

## Description

`BYRO_FSR_FORCE_DISPATCH_FAIL` is documented "Debug-only" but has **no `cfg` gate** (unlike `debug_checking: cfg!(debug_assertions)` next door), and keys on `var_os(..).is_some()`, so `=0` and an empty value both mean "on". Cached in a `OnceLock`, so it cannot be unset for the process — an environment that happens to carry it latches FSR off for the whole session and degrades to the native blit at reduced render extent. Being live in release is arguably *desirable* (smoke and bench run `--release`, which is where the recovery path needs exercising), so the honest defect is the doc plus the predicate. Also undocumented in `docs/engine/fsr3-troubleshooting.md`, whose failure table is where an operator would look.

## Location

`crates/fsr3-sys/src/lib.rs` (`force_dispatch_failure` + its call site)

## Source

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D23-04).

https://github.com/matiaszanolli/ByroRedux/issues/2825
