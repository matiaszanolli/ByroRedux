# SAVE-D2-01: FORMAT_MAJOR tripwire cannot see cfg_attr serde(default); four live fields slipped past

**Issue**: #3020
**Severity**: HIGH
**Dimension**: 2 — format/versioning tripwires
**Labels**: `high,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_SAVE_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SAVE_2026-08-16.md` (Dimension 2 — format/versioning tripwires).

**Location**: `byroredux/src/save_io/serde_default_guard_tests.rs`:83-113 (`serde_attr_declares_default`), :132-165 (the guard test) · live violations at `crates/scripting/src/quest_stages.rs`:105, :108, :79 and `crates/scripting/src/scene/quest_alias.rs`:88

## Description

The `FORMAT_MAJOR` tripwire exists to force a format-version bump when a save-participating field gains `serde(default)` — because a defaulted field silently absorbs a schema change instead of failing loudly.

Its scanner matches **only the bare attribute form**:

```rust
let Some(rest) = trimmed.strip_prefix("#[serde(") else {
    return false;
};
```

The house style throughout this codebase is `#[cfg_attr(feature = "save", serde(...))]`, which **does not start with `#[serde(`** and is therefore invisible to the guard.

## Evidence

Live fields that already slipped past, re-verified 2026-08-17:

```rust
// crates/scripting/src/quest_stages.rs
79:    #[cfg_attr(feature = "save", serde(skip, default))]
105:    #[cfg_attr(feature = "save", serde(default))]   // QuestStageData.status
108:    #[cfg_attr(feature = "save", serde(default))]   // QuestStageData.active
```
```rust
// crates/scripting/src/scene/quest_alias.rs:88
       #[cfg_attr(feature = "save", serde(skip, default))]
```

The guard passes green with four live `serde(default)` declarations in save-participating types.

## Impact

The tripwire that protects save-format compatibility is blind to the attribute form the codebase actually uses — so it is green by construction. Two `QuestStageData` fields (`status`, `active`) already default silently, meaning an older save loads with quest state absorbed to defaults rather than rejected.

This is the guard's entire purpose, defeated by a prefix match.

## Suggested Fix

Match `serde(` **anywhere inside** the attribute, not only as a `#[serde(` prefix — i.e. handle `#[cfg_attr(<cfg>, serde(...))]` as well as `#[serde(...)]`. Then triage the four now-visible violations: either bump `FORMAT_MAJOR` or record why each default is compatible.

## Related

- #3025 (SAVE-D2-2026-08-16-02 — the companion `SAVE_TYPE_SOURCES` gap; together these are why the guard sees so little)
- The `feedback_shader_struct_sync`-style class: a textual guard whose needle is narrower than the house style

## Completeness Checks
- [ ] **ATTR-FORMS**: Both `#[serde(...)]` and `#[cfg_attr(..., serde(...))]` are recognised
- [ ] **BACKLOG**: The four existing violations triaged, not merely made visible
- [ ] **SIBLING**: Any other textual guard in the save suite checked for the same prefix-only assumption
- [ ] **FAILS-LOUDLY**: Adding a `cfg_attr` `serde(default)` to a save type fails the guard
- [ ] **TESTS**: A regression test adds one in each attribute form and asserts both trip

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3020 --json state` when live state is needed.*
