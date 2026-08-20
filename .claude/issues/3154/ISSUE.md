# #3154 — LC-D5-02: `water_material_from_mesh` is an undeclared second `WaterMaterial` producer — watal.md §3's single-site contract is false, and the two `WaterKind` classifiers disagree on `canal`

**Finding**: LC-D5-02
**Labels**: documentation, low, legacy-compat
**Filed**: 2026-08-20 · `/audit-publish` · HEAD `bb0b92f2`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/3154

---

- **Severity**: LOW
- **Dimension**: LEGACY_COMPAT Dim 5 — EXAL / WATAL boundary shape
- **Location**: `byroredux/src/material_translate.rs:90-150` (`water_material_from_mesh`), `:152-176` (`water_kind_from_mesh_name`) vs `byroredux/src/env_translate.rs:912-948` (the WATR classifier); contract text at `docs/engine/watal.md:452-453` (§3 item 1)
- **Status**: NEW

## Description

`docs/engine/watal.md` §3 states the contract for the water boundary:

> **1. Single site.** Both the bulk `--grid` loader and the streaming bootstrap call these — no second construction of `WaterMaterial`/`WaterFlow` anywhere.

**That is no longer true.** Session 70 added the mesh-water slice, and `water_material_from_mesh` constructs a `WaterMaterial` from a NIF `Material` in a different module, reached from the cell and loose-NIF spawn paths.

The design position is defensible — a NIF water mesh has no WATR record, so it genuinely cannot use the per-record translation, and the function's doc comment says so. But the **spec** still claims one site, which removes the auditable invariant: a future third producer has nothing to violate.

The split has already produced one concrete divergence. `WaterKind` is now classified in two places with different token sets:

| Classifier | Tokens |
|---|---|
| `env_translate.rs:930-944` (WATR/ESM path) | `rapid` · `waterfall` · `falls` · `river` · `stream` (+ `NAM5` flow-normal texture, + `NAM0` linear velocity) |
| `water_kind_from_mesh_name` (NIF path) | `waterfall` · `falls` · `rapid` · `river` · `stream` · **`canal`** |

`canal` exists in exactly one of the two, so an asset named for a canal classifies as `River` through the NIF path and `Calm` through the ESM path. The two also disagree on `waterfall`/`falls`, deliberately: `env_translate` demotes those to `River` on horizontal cell planes (a documented anti-fizz fix), while the mesh classifier promotes them to `Waterfall`. That divergence is intentional but is baked into the token match rather than applied by the caller, so it cannot be shared.

## Evidence

`grep -rn "WaterMaterial {"` over the tree returns two production constructors — `byroredux/src/material_translate.rs` (via `WaterMaterial::default()` then field assignment, `:94-149`) and the `env_translate.rs` boundary. The remaining hits are all inside `#[cfg(test)]` blocks: `crates/physics/src/water.rs:906,923,944,977`, `byroredux/src/systems/water.rs:680`, `byroredux/src/commands/water.rs:287,422`, `byroredux/src/systems/character.rs:1389`, `byroredux/src/render/water_wave_params_tests.rs:38`.

`watal.md` §2 *does* describe the mesh-water path in prose (*"Dedicated NIF mesh-water shaders now also cross NIFAL…"*), so this is contract drift **between §2 and §3** of the same document, not undocumented code.

The boundary's other invariants were re-verified and are holding: `resolve_water_material` has exactly two production callers (`byroredux/src/cell_loader/water.rs:364`, `:758`), `default_water_for_worldspace` exactly one (`byroredux/src/cell_loader/exterior.rs:946`), and `WaterFlow::for_kind` remains the single `WaterFlow` constructor — both classifiers call it rather than building the struct.

## Impact

Documentation-vs-code rather than runtime, hence LOW. The cost is **auditability**: the layer's headline invariant no longer describes the layer, so the next audit has nothing to check the tree against. The divergent `canal` token is the first symptom of two classifiers drifting apart, and it is a real (if narrow) behavioural difference today — an asset whose name contains `canal` gets a `WaterFlow` through one path and none through the other.

## Related

- #3152 (LC-D2-01) — a substantive bug inside this same new producer.
- #2790 (CLOSED) — a prior `watal.md` §2 staleness fix; this is §3 and a different claim.
- The `watal.md` §4 payload-table drift is filed separately.

## Suggested Fix

1. Amend `watal.md` §3 to declare **two** boundaries with an explicit split of responsibility — `resolve_water_material` owns WATR-record-backed water, `water_material_from_mesh` owns `*WaterShaderProperty`-backed mesh water — and state that neither may consume the other's inputs. That restores an invariant a future third producer can violate.
2. Hoist the shared `WaterKind` token list into one function both classifiers call, with the horizontal-plane waterfall demotion applied **by the caller** rather than baked into the token match. `canal` then exists once, in one place.

---
*Filed from `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-20.md` (LC-D5-02). Verified against HEAD `bb0b92f2` before filing.*

## Completeness Checks
- [ ] **SIBLING**: `WaterFlow` and `WaterContact` checked for the same undeclared-second-producer pattern (`WaterFlow::for_kind` was clean at audit time — keep it that way)
- [ ] **CANONICAL-BOUNDARY**: hoisting the token list must not move per-game logic downstream of either `translate()` boundary. See `/audit-nifal`.
- [ ] **DOC**: `watal.md` §2 and §3 agree after the change — the drift is between those two sections
- [ ] **TESTS**: a test pins that both classifiers return the same `WaterKind` for the shared token set, including `canal`
