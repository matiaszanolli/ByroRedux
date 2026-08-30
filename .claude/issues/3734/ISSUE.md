# #3734: NIFAL-2026-08-30-D8-02: values() — the walk NIFAL designates the exhaustive lifecycle contract — is the one role walk whose test cannot catch an omission

**Labels**: bug, nif-parser, low, nifal, test-gap
**Filed**: 2026-08-30 (audit-publish)

---

**Report**: `docs/audits/AUDIT_NIFAL_2026-08-30.md` · **Severity**: LOW · **Dimension**: 8 (Shader-flags / Texture roles) · **Tier violated**: no-leak (latent)
**Game affected**: all

## Location
- `crates/nif/src/import/types.rs` — `values()` (currently `:425`) and its test `canonical_iteration_covers_every_role_once` (`:468`); contrast `roles_covers_every_field_in_the_set` (`:1722`)

## Description
`MaterialTextureSet` now carries **three** hand-written role walks plus the generic one:

| Walk | Protected? | By what |
|---|---|---|
| `map_ref` | **yes** — compiler | builds a full struct literal; a forgotten role is a compile error |
| `roles()` (added #3349 this window) | **yes** | `roles_covers_every_field_in_the_set` cross-checks `roles().count()` against `map_ref`'s visit count |
| `values()` | **no** | see below |

`values()`'s test builds a literal with sequential integers and asserts `values() == (0..26)`. That catches a *reordering*, but **not an omission**: add a 23rd role to the struct, give it `26` in the test literal (which the compiler forces you to do), and forget it in `values()` — `values()` still yields the 26 elements `0..=25`, the assert still passes, and the new role is silently absent from every lifecycle consumer.

The fix pattern already exists in the same file, ten lines of it, written three days ago for `roles()`.

## Evidence
The current lists are correct — 22 named roles in the struct, the same 22 in `values()` in the same order, `+ 4` decals = 26; `secondary_values()`'s `skip(1)` correctly assumes `base_color` is element 0. **This is a latent test gap, not a live drop.** Re-verified 2026-08-30: `values()` at `:425`, its sequential-integer test at `:468`, the drift-proof sibling for `roles()` at `:1722`.

## Impact
If it ever fires: `docs/engine/nifal.md` designates `values()`/`secondary_values()` the **exhaustive lifecycle contract** and cell unload uses it directly for texture release, so a role missing from `values()` leaks its texture handle on every cell unload — a compounding GPU resource leak with no compile error and no failing test. It would also silently skip validation and every other exhaustive visit.

## Related
#2697 (`supplemental_texture_indices`, a fourth role walk with no lockstep test — still open), #3465 (the prose-vs-struct parity test, which pins the docs but not `values()`).

## Suggested Fix
Add the six-line sibling of `roles_covers_every_field_in_the_set` — count `map_ref`'s visits and assert `values().count()` equals it. That makes all three walks drift-proof and subsumes the sequential-integer test's omission blind spot.

## Completeness Checks
- [ ] **SIBLING**: #2697's `supplemental_texture_indices` is a fourth walk with no lockstep test — cover it in the same pass
- [ ] **TESTS**: the new assertion must fail if a role is added to the struct and omitted from `values()`
