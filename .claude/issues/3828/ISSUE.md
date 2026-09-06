# DOC-ROT-1: mod-runtime 'no consumer' premise is stale — extensions.rs (10,652 LOC) is now a live consumer

**Severity**: LOW
**Dimension**: 11 — Sandboxed Mod Runtime Trust Boundary
**Location**: `.claude/commands/audit-safety/SKILL.md` Dimension 11 (states mod-runtime "has **no** consumer in the engine yet" — audit it "as a contract, not a live path"); actual live code at `byroredux/src/extensions.rs`; missing row in `.claude/commands/_audit-common.md`'s project-layout map
**Source report**: `docs/audits/AUDIT_SAFETY_2026-09-04.md` (water-deep suite, Dim 11)

## Description
`byroredux/src/extensions.rs` is a real, 10,652-line file (confirmed via `wc -l`), added by commit `24df5304` ("feat(engine): host sandboxed extensions natively") and most recently touched 2026-09-03 — one day before this audit. It is wired into the binary (`mod extensions;` in `byroredux/src/main.rs:28`, called from `main.rs:704` and `main.rs:760`) and directly drives `byroredux_mod_runtime::{SandboxRuntime, SandboxError, ...}` — constructing a live `SandboxRuntime`, calling `compile()`/`instantiate()`, and bridging ECS events/commands to guest WASM components. This flips the premise closed issue #3748 ("`byroredux-mod-runtime` is a dangling `[workspace.dependencies]` alias with no member consumer") established: that was accurate as of its closure, but `extensions.rs` landed afterward.

Compounding this, `.claude/commands/_audit-common.md`'s project-layout map — the shared reference every audit skill is told to trust — has **no entry at all** for `extensions.rs`, even though it individually lists files an order of magnitude smaller (`interaction.rs` 1493 LOC, `inventory.rs` 1008 LOC, `combat.rs` 952 LOC). The file is currently invisible to the whole audit-suite's routing logic, not just this one skill's Dimension 11.

## Evidence
```
$ wc -l byroredux/src/extensions.rs
10652 byroredux/src/extensions.rs
$ grep -n "SandboxRuntime" byroredux/src/extensions.rs | head -3
22:use byroredux_mod_runtime::{... SandboxError, SandboxRuntime};
330:    runtime: SandboxRuntime,
369:            runtime: SandboxRuntime::new(sandbox_config)?,
$ grep -n "mod extensions" byroredux/src/main.rs
28:mod extensions;
$ grep -n "extensions.rs" .claude/commands/_audit-common.md
(no output)
```
Spot-checked (not a full audit of the 10,652 lines): host-registration functions in `extensions.rs` gate on `grants.contains(SCRIPT_FUNCTIONS_REGISTER_CAPABILITY)` / `grants.contains(CONSOLE_REGISTER_CAPABILITY)` (lines 445, 612, 629) — consistent in shape with the check-before-act capability pattern verified directly in `crates/mod-runtime` itself. The `byroredux-mod-runtime` capability catalog and test suite have both grown substantially to support this consumer (test count 23 → 66 since the 2026-08-30 report).

## Impact
An auditor following the skill's current "audit as a contract, not a live path" framing will under-invest scrutiny on a trust boundary that is now live, native, and two days old, with real blast radius (community/guest code driving a 10k-LOC bridge into ECS events/commands). This is a **documentation/coverage-gap finding**, not a confirmed bug in `extensions.rs` itself — this audit did not perform a full line-by-line review of the 10,652-line file; the capability-gating shape spot-checked looks consistent with the established trust model.

## Related
#3748 (closed, established the prior "no consumer" state this finding supersedes).

## Suggested Fix
Update `audit-safety/SKILL.md` Dimension 11 to name `byroredux/src/extensions.rs` as the live consumer and drop the "contract, not a live path" framing; add a layout-map row for it in `_audit-common.md`. Separately (a process suggestion, not a code fix): given its size and freshness, `extensions.rs` is a strong candidate for a dedicated, deeper safety/security pass beyond what a single `/audit-safety` dimension budget covers.

## Completeness Checks
- [x] N/A — documentation-only edit to `.claude/commands/audit-safety/SKILL.md` and `.claude/commands/_audit-common.md`
- [x] **Follow-up flagged, not filed here**: a dedicated deep-dive audit of `extensions.rs` itself (10,652 LOC, untrusted-guest-facing) is recommended as separate future work

## Resolution
`audit-safety/SKILL.md` Dimension 11 was already rewritten in the 2026-09-05 audit-skill sync (a session earlier than this issue's own filing) to name `byroredux/src/extensions.rs` as the live consumer, drop the "contract, not a live path" framing, and cite the 28-capability catalog — verified still in place, no further edit needed there.

The remaining half — the missing `_audit-common.md` layout-map row — was genuinely absent. Added a row for `extensions.rs` next to the other Binary-modules entries (10,652 LOC, `24df5304`, wiring points in `main.rs`/`app_events.rs`, capability-gating summary, pointer to `/audit-safety` Dimension 11 as owner) so it's no longer invisible to audit-suite routing.

The deeper-audit follow-up suggestion is not filed as a separate issue per this pipeline's scope (documentation-only fix); flagging it again here for visibility.

`.claude/commands/_audit-validate.sh` re-run clean after the edit (all paths valid, crate count unchanged).
