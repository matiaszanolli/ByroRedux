**Severity**: LOW · **Dimension**: Audit-infrastructure (doc-rot, same class as the prior audit's Dimension-10 DebugUiState note and the TD4-00x SKILL-staleness issues)
**Source**: `docs/audits/AUDIT_SAFETY_2026-07-25.md` (SAFE-2026-07-25-02)
**Status**: NEW
**Location**: `.claude/commands/audit-safety/SKILL.md` (Dimension 1 and the "Scale of the surface" section), `.claude/commands/_audit-common.md` ("Crate count: 21 under `crates/`" paragraph)

## Description
Dimension 1 of the audit-safety SKILL says "The cxx surface is currently a placeholder... There is no raw-pointer exchange... Do NOT report speculative... findings against this crate." That framing is still accurate for `crates/cxx-bridge`, but `crates/fsr3-sys` (added 2026-07-22, three commits before this audit) is now the codebase's *actual* live FFI boundary: `extern "C"` functions taking `*mut RawContext`/`*const RawCreateDesc`/`*mut RawVersion` etc., a `pub unsafe fn Context::create` with documented pointer/lifetime preconditions, and a `Drop` impl that calls back into the native shim.

`_audit-common.md`'s "21 crates" inventory and coverage-sanity list also don't mention it — there are 22 crates under `crates/` today (confirmed: `audio bgsm bsa core cxx-bridge debug-protocol debug-server debug-ui facegen fsr3-sys nif papyrus pex physics platform plugin renderer save scripting sfmaterial spt ui`), `fsr3-sys` being the addition.

## Evidence
```
$ ls crates | wc -l
22
$ grep -n "Crate count" .claude/commands/_audit-common.md
103:Crate count: 21 under `crates/` — audio, bgsm, bsa, core, cxx-bridge,
$ grep -n "fsr3-sys" .claude/commands/audit-safety/SKILL.md
(no matches)
```
`crates/fsr3-sys/src/lib.rs` (461 lines) exposes `pub unsafe fn Context::create`/`Context::dispatch` with `# Safety` doc sections stating caller contracts (device/physical-device/proc-addr must outlive the `Context`; dispatch handles must belong to the creating device).

## Impact
None to running code — this is a documentation-only gap. Impact is to *future* audits: an agent following the SKILL literally would treat the cxx-bridge scope-guard as covering "the FFI surface" and never grep `fsr3-sys`, exactly as almost happened during the 2026-07-25 safety audit pass (it surfaced only because the total-unsafe-token grep in that audit's step 1 didn't reconcile against the documented per-crate breakdown).

## Suggested Fix
Add `fsr3-sys` to `_audit-common.md`'s crate list (22 crates) and give audit-safety's Dimension 1 a second bullet: "`fsr3-sys` (added 2026-07-22) is a real FFI crossing — every `unsafe fn` needs a `# Safety` doc and lifetime contract; audit it the way Dimension 1 used to reserve for a *hypothetical* live cxx-bridge."

## Related
`docs/audits/AUDIT_SAFETY_2026-07-25.md` — Dimension 1 PASS list already confirms `fsr3-sys`'s current FFI soundness; this issue is purely about the SKILL/common-doc text not yet mentioning the crate exists.

## Completeness Checks
- [ ] **TESTS**: N/A — documentation-only fix, no code path affected
