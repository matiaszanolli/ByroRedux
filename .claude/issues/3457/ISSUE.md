# #3457 — TD4-2026-08-27-05: _audit-common.md Project Layout omits byroredux/src/studio_host.rs and gives crates/sdk no layout row

Labels: `low,tech-debt,doc-rot,documentation`
Filed: 2026-08-28 · Source report: `docs/audits/AUDIT_TECH_DEBT_2026-08-27.md`

---

**Severity**: LOW · **Dimension**: 4 — Audit-Finding Rot · **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-27.md` (TD4-2026-08-27-05)

**Location**: `.claude/commands/_audit-common.md:73-81` (the "Binary modules" row) and the Project Layout block generally; the unlisted code is `byroredux/src/studio_host.rs` (252 LOC) and `crates/sdk/src/` (282 LOC)

**Age**: `21a840d5` (2026-08-25, "feat: introduce byroredux-sdk for renderer-independent tools")

## Description
`crates/sdk` is correctly present in the crate roster (`:142`), the owner map (`:164`) and the un-owned-subsystems table (`:178`) — all three added when it landed — but it has **no entry in the Project Layout block**, and its engine-side consumer `byroredux/src/studio_host.rs` appears nowhere in the file at all. The "Binary modules" row enumerates the binary's top-level files by name and is the layout's authority for "which file owns what"; a 252-LOC new module missing from it is invisible to any audit that scopes itself from that list. Crate count (`25`) is correct and gate-checked.

Filed for the **pattern plus its two remaining instances**, not as a third point fix: a concurrent audit in the same suite run filed the sibling instance (`byroredux/src/commands/physics.rs` missing from the Commands row).

## Evidence
Verified at publish time (2026-08-28):

```
$ grep -n "sdk\|studio" .claude/commands/_audit-common.md
142:nif, papyrus, pex, physics, platform, plugin, renderer, save, scripting, sdk,
164:| `crates/sdk` | no dedicated owner; ... |
178:| ByroRedux SDK | `crates/sdk/src/` | Per-domain owner + `/audit-ecs` ... |
# — three roster/ownership mentions, zero layout rows, and no `studio_host` anywhere.

$ wc -l crates/sdk/src/*.rs byroredux/src/studio_host.rs
    8 crates/sdk/src/lib.rs
  274 crates/sdk/src/studio.rs
  252 byroredux/src/studio_host.rs
```

## Impact
Small individually. The pattern is what matters: the layout block is hand-maintained, is the first thing every audit reads to scope itself, and now has three known gaps from a single fortnight of new modules (`commands/physics.rs`, `studio_host.rs`, `crates/sdk`'s layout row). Both un-listed modules here belong to `crates/sdk`, the subsystem the same file already flags as having no owner audit — so the gap compounds an acknowledged coverage hole rather than sitting beside it.

## Related
The concurrently-filed `commands/physics.rs` layout gap (same mechanism, different row).

## Suggested Fix
Add a `Studio/SDK:` layout row naming `crates/sdk/src/{lib,studio}.rs` and `byroredux/src/studio_host.rs`, and add `studio_host.rs` to the Binary-modules enumeration. Systemically, the crate roster is already gate-checked for count (`_audit-validate.sh:205`); the cheap generalisation is to extend that check to assert every top-level `byroredux/src/*.rs` appears somewhere in the layout block — it is the same shape of check, over a list that drifts at the same rate.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every top-level `byroredux/src/*.rs` and every `crates/*` against the layout block — this finding's two instances plus the concurrently-filed third)
- [ ] **TESTS**: A regression test pins this specific fix (the proposed `_audit-validate.sh` layout-coverage assertion)
