# #3878: TD6-2026-09-05-02: `crates/core/src/stealth.rs` justifies its zero-consumer state with a blocker that shipped — #446 is closed and M42 delivered seven procedure runtimes

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD6-2026-09-05-02) via `/audit-publish`, 2026-09-05. Labels: `low,character,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3878 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD6-2026-09-05-02), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 6 — Stub & Placeholder Implementations
- **Status**: NEW
- **Effort**: trivial (≤30 min)

**Location**: `crates/core/src/stealth.rs` — module docstring, "## Status: greenfield,
math-only, by design" section (`:20-33`)

**Description**

`stealth.rs` is a zero-production-consumer module (`detection_score` / `classify`
have no callers outside their own tests — verified). That is fine *by this
dimension's own rule* — a consumer-less module is expected-and-fine **when documented
as such** — and this one is documented unusually well.

The problem is that the documentation's load-bearing justification is now false. The
docstring reads:

> *"Nothing in the engine feeds this yet: there's no AI-package evaluator, no
> line-of-sight/vision system, no alert-state component, no sneak/crouch flag (see
> the survey behind this module — ROADMAP.md's M42 "AI packages" milestone, which
> this formula will eventually plug into, is Tier 7 and **blocked on `PACK` record
> parsing, #446**)."*

and closes: *"the ECS wiring … **waits until M42 gives it something to drive**."*

Three checkable claims, all stale:

1. **"#446 … blocked on `PACK` record parsing"** — #446 is **CLOSED**
   (`FO3-3-04: PACK AI package records skipped`), closed by `90e6b068` per
   `ROADMAP.md:794`. `crates/plugin/src/esm/records/misc/pack.rs` is 1,895 LOC of
   shipped PACK parsing.
2. **"there's no AI-package evaluator"** — `package_conditions_pass`
   (`byroredux/src/npc_spawn/ai_package.rs:31`) is the M42.2 CTDA package evaluator,
   and `ambient_ai_package_system` (`:572`) is registered unconditionally as a
   `Stage::Update` exclusive at `byroredux/src/boot.rs:1053`.
3. **"waits until M42 gives it something to drive"** — M42 has shipped seven
   procedure runtimes (Sandbox/Wander/Travel/Follow/Escort/Guard/Patrol, M42.1–M42.9;
   `ROADMAP.md:794`).

The *conclusion* (no consumer) remains correct — line-of-sight, alert state and a
sneak/crouch flag genuinely still do not exist. But the stated **precondition has been
met**, so this stub is no longer *blocked*; it is *unscheduled*, and nothing records
that transition. A stub whose deferral condition has silently expired is the exact
case where "documented, therefore fine" stops holding.

**Evidence**

```
$ gh issue view 446 --json state          → {"state":"CLOSED"}
$ grep -n "fn package_conditions_pass\|fn ambient_ai_package_system" byroredux/src/npc_spawn/ai_package.rs
31:fn package_conditions_pass(
572:pub(crate) fn ambient_ai_package_system(world: &World, _dt: f32) {
$ grep -n "ambient_ai_package_system" byroredux/src/boot.rs
1053:    scheduler.add_exclusive(Stage::Update, crate::npc_spawn::ambient_ai_package_system);
```

The module already carries one dated correction of this same class (#2979, *"One
correction to 'nothing feeds this yet'…"*), so the pattern is known here — this is the
next instance of it, not a first offence.

**Impact**

Low but widening. The docstring names `HitEvent.sneak_attack` as "this module's
concrete future hook point"; that field is hardcoded `false` at
`byroredux/src/combat.rs:274` and `:638`. Since the SDK/extension surface landed
(`21a840d5`, 2026-08-25) that constant is now re-exported to sandboxed guest code
through `byroredux/src/extensions.rs:2967,4741` — i.e. it is observable by any mod
loaded via the shipped `--extension` flag, which will see `sneak_attack == false` for
every hit forever. The stub itself is still unreached, which is why this stays LOW.

The concrete harm is misdirection: a contributor reading this module to decide what
to build next is told to go wait on `#446`/M42, both of which are done.

**Related**

- #2979 (CLOSED) — the prior correction to this same "nothing feeds this yet" claim,
  in the sibling `crates/core/src/combat.rs`.
- #3482, #2962 (CLOSED) — prior `stealth.rs` audits; neither touched the M42/#446
  gating claim.
- Overlaps Dim 3 (Stale Documentation). Filed here because Dim 3's discovery recipe
  — path gate, symbol advisory, GPU-size cross-checks — cannot surface an expired
  *deferral condition*, and because the claim's only function is to justify a stub.
  If the merge phase prefers, fold it into Dim 3 rather than reporting twice.

**Suggested Fix**

Replace the `#446`/"Tier 7"/"blocked on PACK record parsing" clause with what is
actually missing today — no line-of-sight/vision system, no `AlertState` component,
no sneak/crouch input — and drop "waits until M42 gives it something to drive"
(M42 has). One sentence; the rest of the section is accurate and should be kept.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
