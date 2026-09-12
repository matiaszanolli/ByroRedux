### SAVE-D2-2026-09-11-01: v22's `FORMAT_MAJOR` bump justification ("discriminant shift") is factually wrong for this codebase's serde_json format — a load-bearing precedent now teaches an incorrect model of what actually requires a bump

- **Severity**: MEDIUM
- **Dimension**: 2 — Registry & (De)serialization Fidelity
- **Data-Loss Class**: None directly (the bump is conservative, not permissive) — but the incorrect stated mechanism is now the citable precedent for future bump/no-bump calls, where the same error in the opposite direction would be a real compatibility bug.
- **Location**: `crates/scripting/src/translate/effects.rs:100` (`enum Effect`, plain `#[derive(Serialize, Deserialize)]`, no tag/repr override); commit `e26579c1` (Fix #3159); baseline comment at `byroredux/src/save_io/serde_default_guard_tests.rs:440-447`; `crates/save/src/snapshot.rs:212` (payload is `serde_json` of `Snapshot`)
- **Status**: NEW
- **Source**: `docs/audits/AUDIT_SAVE_2026-09-11.md`

**Description**: `e26579c1`'s commit message states that adding enum variants "shifts later discriminants under serde's index-based representation" so a pre-v22 snapshot would deserialize as the wrong effect. This is incorrect for the actual on-disk format: `Effect` uses serde's default *externally tagged* JSON representation (confirmed at `effects.rs:99-100`: `#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]`, no tag/repr attribute), which serializes a data-carrying variant as `{"<VariantName>": {…}}` keyed by the Rust variant **name**, never by ordinal. `serde_json`'s `Serializer` ignores the `variant_index` serde_derive passes it — only non-self-describing binary formats care about it, and this codebase doesn't use one for saves. Inserting `SetLocked`/`SetLockLevel` at any position is therefore backward-compatible for deserializing a pre-v22 tail exactly like `Effect::Enable` (#3489, correctly *not* bumped, with the commit's own correct reasoning: "serde only has to recognize the tags actually present"). The two commits give directly contradictory technical justifications for structurally identical changes on the same enum, three commits apart in the same file. Independently corroborated by `docs/audits/AUDIT_INCREMENTAL_2026-09-09.md` §4 for the unrelated `FloatTarget`/`ColorTarget` twin-deletion change, which states the correct principle for the identical format.

**Evidence**: `effects.rs` enum declaration (no tag/repr attribute) confirmed during publish; `snapshot.rs:212`; `#3489`'s own (correct) commit message for the same enum three bumps earlier.

**Impact**: No data loss — the bump fails closed (a clean `UnsupportedVersion` rejection, not silent corruption), consistent with the subsystem's "refuse rather than corrupt" thesis. The cost is (a) every pre-#3159 save was unnecessarily invalidated, working against the subsystem's own "don't make players lose progress" goal, and (b) the precedent-bank comment at `serde_default_guard_tests.rs` — explicitly written for future contributors deciding whether their own change needs a bump — now contains one entry with a wrong stated mechanism, risking either an unnecessary future bump or, more dangerously, an over-generalized "variant insertions are always safe" applied to a case that actually changes an *existing* variant's field shape.

**Related**: `#3489` (the correct precedent this contradicts); `#3159`/`e26579c1` (the commit under review); `docs/audits/AUDIT_INCREMENTAL_2026-09-09.md` §4.

**Suggested Fix**: Correct the `serde_default_guard_tests.rs:440-447` comment and the `e26579c1` record to state the real reason the bump was conservatively taken (not worth reverting now that it shipped). If a genuinely index-sensitive save format is ever adopted, this whole "no bump needed for variant insertion" precedent class needs re-auditing together.

## Completeness Checks
- [ ] **TESTS**: No test change needed — this is a comment-only fix; confirm the existing `saved_type_shape_changes_require_format_major_bump` guard's baseline stays green after the comment edit
- [ ] **SIBLING**: Scan `serde_default_guard_tests.rs`'s other bump-history comments for the same "discriminant shift" framing on an externally-tagged enum
