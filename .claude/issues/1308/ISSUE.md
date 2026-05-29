# #1308 -- OBL-D6-NEW-04: mod-index-01 FormIDs never resolve

_Snapshot as filed 2026-05-28 from /audit-publish (AUDIT_OBLIVION_2026-05-28)._

**Severity**: LOW | **Dim 6** — Blockers & Game-Specific Quirks
**Source**: `docs/audits/AUDIT_OBLIVION_2026-05-28.md` (OBL-D6-NEW-04)

**Location**: `crates/plugin/src/esm/reader.rs:256-277` (FormIdRemap::remap out-of-range arm)

**Issue**: Vanilla `Oblivion.esm` ships a small set of records authored at mod-index 0x01 (a known Bethesda authoring artifact). On a single-plugin load (mod-index 1 > master count = 0), these FormIDs never resolve → silently no-op on cross-references (gameplay/ownership/script/region). No current rendering impact; generates per-form warn spam on verbose loading.

**Suggested fix**: in `remap()`, when `mod_index > master count` on a single-plugin load, recognize the Oblivion index-01 case and either clamp to the self plugin_index (treat as self-reference) or tag the form as engine-injected and suppress the per-form warn.

## Completeness Checks
- [ ] **SIBLING**: check FO3/FNV for the same Bethesda index-01 artifact
- [ ] **TESTS**: unit test asserting index-01 FormIDs are handled without warn spam
- [ ] **CANONICAL-BOUNDARY**: ESM parse-side only
- [ ] **UNSAFE**: no unsafe involved
