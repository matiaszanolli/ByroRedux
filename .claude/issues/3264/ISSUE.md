# 3264: CONC-D3-2026-08-24-04: resource accessors defuse the lock-tracker scope before constructing the guard

**Severity**: LOW · **Report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-24.md` (CONC-D3-2026-08-24-04)

## Description

All four `Query` accessors construct the wrapper first, defuse after (the #2149 fix). All six `Resource` accessors do the opposite — benign today only because `ResourceRead::new`/`ResourceWrite::new` are pure struct literals.

## Location

`crates/core/src/ecs/world.rs:687-688`, `:716-717`, `:772-777`, `:786-792`, `:814-815`, `:833-835`

## Impact

No live defect — latent regression risk if a future hot-path optimization (#1367-style) makes `ResourceRead::new`/`ResourceWrite::new` fallible.

## Related

#2149, #137, #1367.

## Suggested Fix

Reorder all six to construct-then-defuse (zero cost, both infallible today), or add a one-line note at each site.

## Completeness Checks
- [ ] **LOCK_ORDER**: All six sites match the Query-accessor pattern
