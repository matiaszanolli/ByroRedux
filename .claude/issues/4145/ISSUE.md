### SAVE-D6-2026-09-11-01: extensions preflight/restore are never named in `save-load-roundtrip.md` §6's numbered ordering list, only in a disconnected prose section

- **Severity**: LOW
- **Dimension**: 6 — M45.1 Live Load-Apply (documentation)
- **Data-Loss Class**: none (doc rot / doc gap)
- **Location**: `docs/engine/save-load-roundtrip.md:138-236` (§6's 8-step numbered list) vs. `:261-292` ("Engine-native extension state")
- **Status**: NEW. `24df5304` (sandboxed extensions) landed after the 2026-08-30 audit; the doc section was added in the same era but never folded into §6's step list.
- **Source**: `docs/audits/AUDIT_SAVE_2026-09-11.md`

**Description**: Neither `crate::extensions::preflight_extension_state` (real step 3, between the typed preflight and the #3789 pre-reload `restore_resources`) nor `crate::extensions::restore_extension_state` (real step 7, between the reload and the post-reload `restore_resources`) appears in §6's numbered trace — confirmed by reading the full 8-step list during publish. The separate "Engine-native extension state" section covers the right invariants in prose but names neither function nor its position relative to §6's steps.

**Impact**: Documentation only — the substance is accurate in isolation, but a reader following §6 to understand control flow has no way to learn these two calls exist or where they sit. This is exactly the kind of gap that let a real ordering bug (`SAVE-D6-2026-08-30-01`) through three audit cycles unnoticed.

**Related**: `24df5304`; `SAVE-D6-2026-08-30-01`/`-04` (the closed findings this section is adjacent to).

**Suggested Fix**: Fold two bullets into §6's numbered list — "1b. Extensions preflight" between steps 1 and 2, "6b. Extensions restore" between the reload and "Restore whole resources" — each cross-referencing the existing prose section rather than duplicating it.

## Completeness Checks
- [ ] **TESTS**: N/A — documentation-only fix
