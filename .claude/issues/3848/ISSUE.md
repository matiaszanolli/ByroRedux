# #3848: TD6-2026-09-05-01: `skyrim_ruleset` / `oblivion_ruleset` are production-unreachable — `build_ruleset` silently returns `None`, and #3170's landed fix never reached `main`

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD6-2026-09-05-01) via `/audit-publish`, 2026-09-05. Labels: `medium,character,game:skyrim,game:oblivion,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3848 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD6-2026-09-05-01), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: MEDIUM (promoted from the LOW default — reachable from a shipped CLI flag *and* a smoke test; reachability traced below)
- **Dimension**: 6 — Stub & Placeholder Implementations
- **Status**: **Regression of #3170** (closed 2026-08-25; fix present only on an unmerged branch)
- **Effort**: small (≤2 h) — the fix is already written; the work is merge + re-verify
- **Age**: `bbd501a1` (2026-08-25) is the commit that *would* have fixed it; the gap itself predates CHARAL's profile split

**Location** (symbol-anchored)

| Symbol | File |
|---|---|
| `enum RulesetBuilder` (`:47`) | `crates/core/src/character/profile.rs` |
| `CharacterRulesProfile::OBLIVION` (`:82`) / `::SKYRIM` (`:119`) | `crates/core/src/character/profile.rs` |
| `CharacterRulesProfile::build_ruleset` (`:178`) | `crates/core/src/character/profile.rs` |
| `skyrim_ruleset` (`:116`) | `crates/core/src/character/skyrim.rs` |
| `oblivion_ruleset` (`:101`), `oblivion_pool_regen_config` (`:159`) | `crates/core/src/character/tes.rs` |
| `build_character_ruleset` (`:228`) | `byroredux/src/npc_spawn.rs` |

**Description**

`RulesetBuilder` has four variants — `None`, `Fallout3`, `FalloutNewVegas`,
`Fallout4`. There is **no `Skyrim` and no `Oblivion` arm**. Both
`CharacterRulesProfile::SKYRIM` and `::OBLIVION` therefore carry
`ruleset: RulesetBuilder::None`, and `build_ruleset` hits:

```rust
RulesetBuilder::None => return None,
```

That is the silent-stub shape this dimension hunts: no panic, no `log::warn!`, no
`TODO` — just a `None` that propagates all the way to "the resource was never
inserted". Meanwhile `skyrim_ruleset()` and `oblivion_ruleset()` are **complete,
tested builders** sitting one module away with **zero production call sites**
(only their own unit tests and `crates/plugin/src/esm/records/tests.rs`).

`skyrim_ruleset` is not merely written — it is verified against real game data.
`crates/plugin/src/esm/records/tests.rs` builds it from a live `Skyrim.esm` AVIF
index and asserts `rs.derived_row_len() == 2` with the message *"one or more
Skyrim.esm AVIF EditorIDs failed to resolve"*. The builder works; nothing calls it.

**The closed-issue twist.** #3170 (`CHAR-2026-08-20-D3-01`, MEDIUM) named this exact
`RulesetBuilder`-has-no-`Skyrim`-arm gap and was closed 2026-08-25 by commit
`bbd501a1`, whose subject reads *"Fix #3170: **wire a Skyrim RulesetBuilder arm** so
#2942's GMST-sourcing seam reaches production"*. That commit is **not on `main`**:

```
$ git merge-base --is-ancestor bbd501a1 main && echo YES || echo NO
NO
$ git branch -a --contains bbd501a1
  fix/npc-spawn-dead-code-oblivion-ignore-charal-gmst
  remotes/origin/fix/npc-spawn-dead-code-oblivion-ignore-charal-gmst
```

`main` at HEAD still has the four-variant enum. The sibling fix in the same commit
(#3169, `SkillSet::SKYRIM` Illusion → `AVMysticism`) **did** reach `main` via another
route (`crates/core/src/character/skill.rs:123,141-149`), so this is a partially-applied
branch, not a wholly-forgotten one — which is exactly why it reads as done.

**Evidence — reachability trace (not asserted, walked)**

1. Shipped CLI flags `--game skyrim` / `--esm Skyrim.esm` (both in `README.md#run`)
   → `parse_esm` → `crates/plugin/src/esm/records/mod.rs:156`:
   `GameKind::Skyrim => CharacterRulesProfile::SKYRIM`.
2. `byroredux/src/cell_loader/references/mod.rs:308-313` — the once-per-load CHARAL
   construction site — calls `crate::npc_spawn::build_character_ruleset(record_index)`
   and inserts only `if let Some(rs)`.
3. `build_character_ruleset` → `index.character_rules.build_ruleset(resolve, gmst)`
   → `profile.rs:187` `RulesetBuilder::None => return None`.
4. No `CharacterRuleset` resource is ever inserted on Skyrim or Oblivion.

Smoke-test reachability: `docs/smoke-tests/p2-melee-core.sh` defaults to
`skyrim_se` (`# ... default \`skyrim_se\``, line 11) and is the gate ROADMAP cites for
"P2 combat core landed 2026-08-16".

Downstream consumers all degrade silently through the same `let … else` shape:

- `byroredux/src/combat.rs::melee_damage_charal_bonus` (`:453`) → `return 0.0`
- `crates/core/src/character/regen.rs::pool_regen_tick_system` (`:152`, a registered
  `Stage::Update` exclusive at `byroredux/src/boot.rs:1130-1140`) → `return`
- `crates/scripting/src/condition.rs:500,671` — the CTDA `GetActorValue` derived-stat
  fallback

**Second symbol in the same cluster** (reported here rather than double-filed under
Dim 8, per cross-dimension dedup): `oblivion_pool_regen_config`
(`crates/core/src/character/tes.rs:159`) has **zero call sites anywhere in the
workspace — not even a test**. The only three references are its definition, the
`pub use` re-export in `crates/core/src/character/mod.rs:121`, and a doc link at
`regen.rs:122`. Both `insert_resource(PoolRegenConfig { … })` sites in `regen.rs`
(`:255`, `:396`) are inside `#[cfg(test)] mod tests` (opens at `:224`), so
`PoolRegenConfig` is never inserted in production on **any** game and
`pool_regen_tick_system` is a permanent no-op engine-wide.

**Impact**

On Skyrim SE — the game the P2 vertical slice, `p2-melee-core.sh`,
`p1-character-traversal.sh` and `p0-door-interaction.sh` all target:

- No Magicka/Stamina regen (`pool_regen_tick_system` inert).
- No derived-stat rows for CTDA `GetActorValue`, so condition-gated content that
  asks for a derived value silently reads nothing.
- `LevelingModel::with_gmst` never runs — Skyrim's `SkillXp` variant is the *only*
  arm `with_gmst` handles, so #2942's whole GMST-sourcing seam has zero production
  reach on every shipped game (this is #3170's original subject, still true at HEAD).

Blast radius is bounded — nothing crashes, and `docs/feature-matrix.md:250,258-260`
records the state accurately ("~ built, unwired"; "`RulesetBuilder` enum has no
Oblivion/Skyrim arm"). The debt is that a **written, reviewed, issue-closing fix is
sitting unmerged** while the issue reads CLOSED, so no tracking surface will ever
surface it again.

**Related**

- #3170 (CLOSED 2026-08-25) — the issue this regresses; its fix is the unmerged commit.
- #3768 (CLOSED) — documented the *Oblivion* half as doc rot only. Its own
  Completeness Check names the unchecked sibling: *"the Skyrim paragraph in the same
  §5, which has the same `RulesetBuilder::None` shape"*. That sibling check is what
  this finding closes.
- #2941 (CLOSED) — same defect class, previously fixed for FO3
  (*"fallout3_ruleset and LevelingModel::FO3 are unreachable"*).
- User memory `orphan_branch_unmerged_fixes.md` records #2266/#3084/#3170/#3169 as
  closed-with-unmerged-fixes; this finding is the code-level confirmation for #3170,
  and shows #3169 is *not* affected. **Cross-dimension note for the merge phase**:
  #2266 (dead NPC-spawn wrappers) and #3084 (Oblivion corpus ignore-gate) belong to
  Dim 8 and Dim 9 respectively and should be re-verified there — #2266's wrappers
  appear absent from `main`, so the branch is partially applied and each issue needs
  checking on its own, not as a block.

**Suggested Fix**

Cherry-pick `bbd501a1`'s `RulesetBuilder::Skyrim` arm onto `main` (add the variant,
map `CharacterRulesProfile::SKYRIM.ruleset` to it, dispatch to `skyrim_ruleset`),
then re-run `p2-melee-core.sh`. Oblivion stays `None` on purpose — per #3768 it is
additionally blocked on a pre-`AVIF` legacy actor-value resolver — but that arm's
absence should carry a one-line comment saying so, so the next reader does not read
`RulesetBuilder`'s shape as an oversight in both directions. Separately, either wire
`oblivion_pool_regen_config` or delete it; a `pub fn` with zero call sites including
tests is unverified code.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
