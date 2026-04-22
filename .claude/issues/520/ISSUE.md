# FNV-ESM-9: PerkRecord is stub, PRKE entry points unparsed

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/520
- **Severity**: MEDIUM
- **Dimension**: ESM record parser
- **Audit**: `docs/audits/AUDIT_FNV_2026-04-21.md`
- **Status**: NEW (created 2026-04-21)

## Location

`crates/plugin/src/esm/records/misc.rs:524-550`

## Summary

`parse_perk` reads only EDID/FULL/DESC/first DATA byte. PRKE/PRKC/DATA entry-point blocks (damage bonuses, skill-check overrides, condition gates) are deferred. FNV perk modifiers cannot apply.

Fix: extend `PerkRecord` with `Vec<PerkEntry>` when perk-entry-point condition pipeline lands.

Fix with: `/fix-issue 520`
