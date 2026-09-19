# FO3-D3-2026-09-19-01: FO3-D3-2026-09-19-01: INFO `DATA` (dialogue Type + Flags 1) has no decode arm — dropped on 100% of FO3 INFOs; shared with FNV → /audit-esm routing

- **Labels**: low,bug,esm-plugin,game:fo3,legacy-compat
- **Filed from**: docs/audits/AUDIT_FO3_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4469

---

**Dimension**: 3 — ESM Record Coverage
**Filed from**: `docs/audits/AUDIT_FO3_2026-09-19.md` (/audit-fo3, HEAD `340799d66`). Shared-mechanism defect (identical on FNV) — routed per the /audit-esm scope split; filed here so it is tracked at all.

**Description**

Every FO3 INFO carries a `DATA` sub-record (measured 2026-09-19: **22,327/22,327 FO3; 23,247/23,247 FNV**) that per xEdit's FO3 definition (`wbDefinitionsFO3.pas` `wbINFOAfterLoad` — reads `DATA\Flags 1` bit `$80` to decide DNAM retention and rewrites `DATA\Type` 3→0) holds at least the dialogue `Type` byte and a `Flags 1` word (goodbye / random / say-once response-flag class). `parse_info` (`crates/plugin/src/esm/records/misc/dialogue.rs:186+` — 13 subcode families: NAM1/NAM2/TRDT/TRDA/TCLT/NAME/TCLF/PNAM/ANAM/CTDA/CTDT/CIS1/CIS2) has **no `DATA` arm**, so the field is silently discarded.

**Evidence**: raw census counts above; arm list from `parse_info` source; `DialRecord`'s own `DATA` *is* decoded (`dial_type`, byte 0, `dialogue.rs:179`) — the INFO-side omission looks like an oversight rather than a policy, since the module docs' deliberate-deferral list (conditions / scripts / edits; conditions since landed via #3614) never mentions `DATA`.

**Impact**: latent — no dialogue runtime consumes response flags yet (result scripts SCHR/SCDA likewise deferred, #631). When a dialogue runtime lands, it cannot tell a goodbye-flagged line from a plain one without re-parsing the file. Identical on FNV.

**Related**: #631 (INFO stub deferrals), #3614 (CTDT/TCLF/NAME arms), /audit-esm.

**Suggested Fix**: add a `DATA` arm to `parse_info` storing byte 0 as `info_type` (the same byte-0-is-type convention `DialRecord::dial_type` already uses) and the flags word raw; update the deferral list in the module docs.

## Completeness Checks
- [ ] **SIBLING**: While adding the arm, confirm no other INFO subform in the FO3/FNV census (53 distinct REFR-side kinds checked; DIAL/INFO families) is silently undecoded the same way
- [ ] **TESTS**: A fixture with a `DATA` sub-record asserts `info_type` + flags decode (and the existing parse_info tests stay green)
