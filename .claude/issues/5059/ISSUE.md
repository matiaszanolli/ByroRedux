# #5059 — SAVE-D2-2026-09-29-01: `save_type_sources()` is blind to nested payloads of two registered types — 573170e1c dropped `crates/core/src/lighting.rs` (`LightSource.emitter`), and `crates/sdk` payloads (`FormRef` in `FactionRelations`, `ScriptValue` in `PapyrusProviderContinuationQueue`) were never scanned

**Labels**: medium,save-load,test-gap,bug

**Source report**: `docs/audits/AUDIT_SAVE_2026-09-29.md`
**Severity**: MEDIUM
**Dimension**: Format & Schema Discipline — data-loss class: none today (latent guard gap)

## Location
- `byroredux/src/save_io/serde_default_guard_tests.rs` — `save_type_sources` (keeps a file only if it contains `cfg_attr(feature = "save"` or defines a registered name; explicit list has four files)
- `crates/core/Cargo.toml` — core serde derives are gated on `inspect` (`save = ["inspect"]`)
- `crates/core/src/ecs/components/light.rs` — `pub emitter: Emitter`
- `crates/core/src/lighting.rs` — `Emitter`, `AttenuationModel`, `VisibilityMask`, `RadiantIntensityRgb`, `Meters`
- `crates/sdk/src/identity.rs` — `FormRef`; `crates/sdk/src/script_function.rs` — `ScriptValueType`, `ScriptValue`
- `crates/scripting/src/combat.rs`, `crates/scripting/src/papyrus_provider/ir.rs`

## Description
Commit 573170e1c (titled "feat(tests): add new test for concurrent initialization of OnceLock") removed the `cfg_attr(feature = "inspect"` arm from the file filter, reasoning that feature-gated save derives cover standalone serialized payloads. But every `byroredux-core` derive is `inspect`-gated, not `save`-gated. A reproduction of the filter shows 16 core files dropped; only `lighting.rs` holds a nested payload of a registered column (`LightSource`, which is also in `MUTABLE_DELTA_COLUMNS`). The `crates/sdk` payloads of `FactionRelations` (new at v28) and `PapyrusProviderContinuationQueue` were never selected at all. Fifth recurrence of the #2015 / #2537 / #3025 / #3167 class.

## Evidence
`crates/core/src/lighting.rs` and `crates/sdk/src/identity.rs` each contain 0 `cfg_attr(feature = "save"` occurrences; `lighting.rs` derives are `cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))`. Neither defines a registered type name, and neither is in the explicit list. No shape change has slipped through yet (lighting.rs diffs since the narrowing change only defaults/const fns; `FormRef` unchanged since v28).

## Impact
A new or retyped field in `Emitter`, `FormRef` or `ScriptValue` would ship without a `FORMAT_MAJOR` bump; old saves would then fail `serde_json` decoding after the version gate passed them, or the typed preflight would reject them with a confusing error.

## Related
#2015, #2537, #3025, #3167, #4141.

## Suggested Fix
Add `crates/core/src/lighting.rs`, `crates/sdk/src/identity.rs` and `crates/sdk/src/script_function.rs` to the explicit nested-payload list. Better: derive the list from field types of registered types, or restore the `inspect` sweep and allowlist its known false positives.

Validated at HEAD 9fcfdc3fc: `save_type_sources` filter retains only `cfg_attr(feature = "save"` files or registered-type definers plus four explicit paths; `lighting.rs`/`identity.rs` match neither.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
