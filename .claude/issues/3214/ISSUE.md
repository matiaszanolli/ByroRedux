# REG-2026-08-20-D1-02: #3038 single-normaliser invariant violated by two other registry-key producers

**Issue**: #3214 — https://github.com/matiaszanolli/ByroRedux/issues/3214
**Severity**: LOW
**Labels**: `low,import-pipeline,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_REGRESSION_2026-08-20.md`
**Filed**: 2026-08-20 · `/audit-publish` · verified against HEAD `bb0b92f2`

---

Filed from `docs/audits/AUDIT_REGRESSION_2026-08-20.md` § REG-2026-08-20-D1-02 (Dimension 1 — Closed-issue discovery & fix presence).

**Severity**: LOW (latent, not live)
**Location**: `byroredux/src/cell_loader/nif_import_registry.rs:33-49` (the invariant + `canonical_model_path_key`), `:85-90` (the forward-slash test); violators at `byroredux/src/streaming_helpers.rs:366` and `byroredux/src/cell_loader/partial.rs:25`.

This finding block contains **two distinct defects**. Both are listed below; neither should be dropped when fixing the other.

## Defect 1 — two registry-key producers bypass the single-normaliser invariant

#3038's fix doc is explicit (`nif_import_registry.rs:40-42`):

> *"#3038 / FNV-2026-08-16-D1-02 — **every producer of a registry key MUST route through this one function** rather than building the key inline."*

Two `NifImportRegistry` key producers still build the key inline with a bare `model_path.to_ascii_lowercase()`:

```rust
// streaming_helpers.rs:366 — negative-cache producer
let cache_key = model_path.to_ascii_lowercase();
let mut reg = world.resource_mut::<cell_loader::NifImportRegistry>();
reg.insert(cache_key, None)

// partial.rs:25 — positive-cache producer
let cache_key = model_path.to_ascii_lowercase();
```

Neither file imports `canonical_model_path_key`:

```
$ grep -n "canonical_model_path_key" byroredux/src/streaming_helpers.rs \
      byroredux/src/cell_loader/partial.rs byroredux/src/streaming.rs
byroredux/src/streaming.rs:37: use crate::cell_loader::{canonical_model_path_key, …};
byroredux/src/streaming.rs:1243:  let key = canonical_model_path_key(model_path);
```

Both are correct **today** only because the only caller chain feeds them a string `pre_parse_cell` already canonicalised at `streaming.rs:1243` — the lowercase is a no-op on an already-lowercase key and the `meshes\` prefix rides along. **Nothing asserts that precondition, and neither function's signature expresses it.**

The five #3038 tests all exercise the helper directly; **none reaches either call site**, so reverting `streaming.rs:1243` to a bare lowercase — the precise pre-fix state — leaves all five green.

## Defect 2 — a checked-in test pins the separator split as *correct*

`canonical_model_path_key` deliberately does **not** unify the two separator forms:

- `Meshes\Clutter\x.nif` → `meshes\clutter\x.nif`
- `meshes/clutter/x.nif` → `meshes/clutter/x.nif` (unchanged)

and `does_not_double_prefix_forward_slash_form` (`:85-90`) **pins that divergence as correct** — in the very module whose reason to exist is *"the same asset must not land under two keys."*

```rust
assert_eq!(
    canonical_model_path_key("meshes/clutter/barrel02firelight.nif"),
    "meshes/clutter/barrel02firelight.nif"
);
```

## Impact

Low today — latent, not live. The reachable failure is a **future producer**, or a change to what `payload.parsed` carries, silently re-splitting the cache key space. That is #3038's original symptom: assets parsed and imported twice, cache-hit telemetry undercounting reuse.

## Not a defect — checked and cleared

`byroredux/src/cell_loader/precombined.rs:344` is a *third* inline producer but is genuinely safe: `precombine_oc_nif_path` synthesises a deterministic lowercase-hex `meshes\precombined\…` path **in its own namespace** that no authored `model_path` can reach. Self-consistent; recorded so the next sweep does not re-derive it.

## Suggested Fix

1. Route both violators through `canonical_model_path_key`. It is documented **idempotent**, so this is a safe no-op today and a real guard tomorrow.
2. Either normalise `/` → `\` inside the helper, **or** rewrite `does_not_double_prefix_forward_slash_form` to assert *convergence* rather than pinning the split.
3. Add a guard that reaches at least one *call site* rather than only the helper, so reverting `streaming.rs:1243` fails something.

## Related

- **#3038** — the invariant this violates; `bff2d5a3`
- **#862** / **#864** — the cache-key snapshot the invariant protects
- The global instruction *"always prioritize improving existing code rather than duplicating logic"*

## Completeness Checks
- [ ] **BOTH-DEFECTS**: The inline producers *and* the separator-split test are both addressed — fixing one leaves the other live
- [ ] **SIBLING**: `grep -rn "to_ascii_lowercase" byroredux/src/cell_loader byroredux/src/streaming_helpers.rs` shows no remaining inline registry-key producer (`precombined.rs:344` excepted and documented as namespaced)
- [ ] **TESTS**: A guard reaches a call site — reverting `streaming.rs:1243` to a bare lowercase makes something fail
