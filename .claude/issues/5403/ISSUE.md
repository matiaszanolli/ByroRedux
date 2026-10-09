# #5403: SAVE-D2-2026-10-08-01: the shape-fingerprint guard reads only the last `#[…]` attribute before a declaration, so `StoryManagerNodeState` and `SmNodeRuntime` (save derive placed above a plain `#[derive(Debug…)]`) are invisible to it

**Labels**: medium,save-load,bug,test-gap
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5403

**Source**: `docs/audits/AUDIT_SAVE_2026-10-08.md` — `SAVE-D2-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `normalized_serialized_shapes` uses `prior.rfind("#[")`; both SM types put the save `cfg_attr` above a plain `#[derive(Debug, …)]`.

- **Severity**: MEDIUM
- **Dimension**: Format & Schema Discipline
- **Data-Loss Class**: irrecoverable-write (latent). A future shape change without a bump makes same-major saves fail the typed preflight.
- **Location**:
  - `byroredux/src/save_io/serde_default_guard_tests.rs:241-253` (`prior.rfind("#[")`, then the derive/Serialize test on that one span);
  - `crates/scripting/src/story_manager.rs:161-181` (`#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]` followed by `#[derive(Debug, Clone, Default, PartialEq)]` on both types).
- **Status**: NEW. It is a different mechanism from #5059: that issue is about files outside the scan, while this one is about types inside a scanned file. Same family as the closed #3164.
- **Description**:
  - `normalized_serialized_shapes` takes the *nearest* `#[` above each `struct`/`enum` line and requires that one span to contain both `derive` and a Serialize needle.
  - For both new Story Manager types, the nearest span is the plain `#[derive(Debug, Clone, Default, PartialEq)]`, so they are skipped. `story_manager.rs` is in `save_type_sources()` (it carries a `cfg_attr(feature = "save"` derive and defines a registered type), but it contributes no shape.
  - Empirical confirmation: `26b6a779c` added two serde-derived types to a scanned file and did not refresh `BASELINE_SHAPE_FINGERPRINT`, yet the guard stayed green. Compare `14cff35ae`: `DialogueSpokenInfoForms` has the derives in the other order, and that commit had to refresh.
  - A Python emulation of the guard's attribute test over every workspace `.rs` file finds these two as the only in-scan saved types with the blind ordering. The other hits are sdk and inspect-only types outside the scan, the known #5059 territory.
- **Impact**: a field added to, retyped in, or newly `Option`-wrapped inside `SmNodeRuntime` or `StoryManagerNodeState` passes every guard without a `FORMAT_MAJOR` bump. The `serde(default)` guard would still catch a defaulted field. Saves of the same major then fail `validate_snapshot_types` with a decode error instead of a clean version refusal. Any future type that copies this attribute order joins the blind spot silently.
- **Suggested Fix**:
  - Treat the whole contiguous attribute block above the declaration as `between`, not just the last `#[`.
  - Add a coverage assertion: every registered type defined in a scanned file must contribute a shape.
  - Refresh the baseline in the same commit, as a refresh without a bump with a justification comment. The two types join the hash and have not changed shape since registration.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the `serde_default` guard's own attribute scan, which may share the nearest-`#[` heuristic)
- [ ] **TESTS**: A regression test pins this specific fix
