# #3439 — TD4-2026-08-27-02: _audit-validate.sh skips every backticked bare basename — a deleted file is invisible to the gate, which is why ten dead ai.rs refs pass

Labels: `low,tech-debt,bug`
Filed: 2026-08-28 · Source report: `docs/audits/AUDIT_TECH_DEBT_2026-08-27.md`

---

**Severity**: LOW · **Dimension**: 4 — Audit-Finding Rot · **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-27.md` (TD4-2026-08-27-02)

**Location**: `.claude/commands/_audit-validate.sh` — `should_skip()` (first skip rule)

## Description
The gate's first skip rule discards any reference without a `/`:

```bash
should_skip() {
    local p="$1"
    # Bare basenames (`lib.rs`, `systems.rs`, `tests.rs`) are used as
    # shorthand inside a paragraph that already established the dir
    # context. They carry no path info to begin with, so they can't
    # go stale in the "wrong dir" sense this gate targets.
    [[ "$p" != */* ]] && return 0
```

The stated rationale is sound for its stated case and wrong for the case that actually occurred. A bare basename carries no *directory* information, but it still asserts **existence** — and the machinery to check exactly that is already in the file two functions later:

```bash
path_exists() {
    local p="$1"
    [[ -e "$p" ]] && return 0
    grep -qE "(^|/)${p//./\\.}\$" "$all_paths_file"   # path-suffix match
}
```

`path_exists "ai.rs"` returns false today, so the gate has everything it needs and is prevented from using it by the skip. #3202 extended the gate to `docs/engine/*.md` precisely so reference docs get "the same policing as the skills"; this blind spot means ten dead citations in one such doc still pass silently.

## Evidence
Verified at publish time (2026-08-28):

```
$ .claude/commands/_audit-validate.sh | tail -1
OK: all path references valid.

$ git ls-files | grep -E "(^|/)ai\.rs$" ; echo "exit=$?"
exit=1

$ grep -c '`ai\.rs' docs/engine/npc-spawn-ai-packages.md
10
```

## Impact
One structural class of doc rot — *the file was deleted, not moved* — is unreachable by the gate, in exactly the tier #3202 added to close that hole. Low blast radius (documentation only) but it defeats the gate's purpose on its newest and least-reviewed input set.

## Related
#3202 (CLOSED — extended the glob to `docs/engine/`, the change this finding completes); #3197 (CLOSED — two earlier gate blind spots); the `npc-spawn-ai-packages.md` rot filed alongside this report (the rot this blind spot hid).

## Suggested Fix
Replace the unconditional skip with a conditional one — skip a bare basename only when `path_exists` succeeds (then it is genuinely shorthand); report it when it resolves nowhere in the tree. Two lines. Expect a small first-run advisory backlog of legitimately-generic names (`lib.rs`, `mod.rs`, `tests.rs` all resolve, so they stay silent).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other `should_skip` rules — each should be re-read for the same "no directory info ≠ no assertion" confusion)
- [ ] **TESTS**: A regression test pins this specific fix (a fixture doc citing a deleted bare basename must fail the gate)
