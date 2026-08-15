# CHAR-D3-04: level_cap has no consumer, and its docstring describes a DLC bump no loader performs

- **Issue**: [#2943](https://github.com/matiaszanolli/ByroRedux/issues/2943)
- **Finding ID**: `CHAR-D3-04`
- **Labels**: `low,legacy-compat,tech-debt,bug`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2943 --json state`.

---

- **Severity**: LOW
- **Dimension**: Leveling & Progression
- **Game**: FO3 / FNV (the only capped models)
- **Location**: `crates/core/src/character/leveling.rs:43-50` and `:201-210`; sole model consumer at `crates/scripting/src/condition.rs:574-585`
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md` § *XP / level curve* — "**Level cap:** FO3 **20** (30 with *Broken Steel*); FNV **30** (50 with the four add-ons, +5 each)"
- **Description**: `level_cap()` is correct and its `0 = uncapped` sentinel is structurally un-divergable (one merged match arm across all three variants), but nothing calls it — not `GetXPForNextLevel`, not the spawn path, not any test beyond `level_caps_per_game`, which only reads the raw stored values back. The doc comments assert behavior that does not exist: `"a hard `level_cap` (`0` = uncapped; add-ons raise it)"` and `"Add-ons raise it; the loader bumps it when DLC is present."` A repo-wide grep finds no code that raises a level cap for any DLC, and `build_character_ruleset` does not inspect the load order for add-ons at all. Consequently `GetXPForNextLevel` on an actor at or above the cap returns a positive XP requirement rather than reflecting the cap. I deliberately do not assert what the capped return *should* be — no capture document states it (`feedback_no_guessing`).
- **Evidence**:
  ```rust
  // crates/core/src/character/leveling.rs:201-202 — a claim about the loader
  /// The base-game hard level cap (`0` = uncapped). Add-ons raise it; the
  /// loader bumps it when DLC is present.
  ```
  ```rust
  // crates/scripting/src/condition.rs:584 — the only model consumer, cap-blind
  rs.leveling.xp_to_next(level)
  ```
  `grep -rn "level_cap"` outside `leveling.rs` returns nothing.
- **Impact**: Small today — one CTDA returns a vanilla-curve value past the cap. The real cost is the docstring: it reads as a description of shipped behavior and will be believed by whoever builds the leveling runtime, who then will not implement the DLC bump because they think the loader already does.
- **Related**: CHAR-D3-02 (the same accessor set is where the FO3/FNV divergence hides)
- **Suggested Fix**: Reword both doc comments to the imperative ("add-ons *should* raise it; not yet implemented") or file the DLC-bump work, and have `xp_to_next`'s caller consult `level_cap()` once the capped semantics are sourced.

## Completeness Checks
- [ ] **SIBLING**: The same pattern is checked in the other per-game ruleset builders (`fallout.rs` / `tes.rs` / `skyrim.rs`), not just the one cited
- [ ] **SOURCE**: Any changed constant cites the capture document line it comes from (`docs/engine/charal-*-ruleset.md`) — never a guessed value
- [ ] **CHARAL-BOUNDARY**: The per-game seam stays *data in the tables*; no consumer gains a branch on game identity
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
