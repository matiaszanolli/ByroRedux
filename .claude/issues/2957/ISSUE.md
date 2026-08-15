# CHAR-D5-04: the auto-calc deferral note under-states its scale (40% of FNV actors, not a tail)

- **Issue**: [#2957](https://github.com/matiaszanolli/ByroRedux/issues/2957)
- **Finding ID**: `CHAR-D5-04`
- **Labels**: `low,legacy-compat,documentation`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2957 --json state`.

---

- **Severity**: LOW
- **Dimension**: Population Boundary
- **Game**: fnv, fo3
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs` (module docstring,
  "Deferred (intentionally, not guessed)" → the *Non-auto-calc NPCs* bullet) ·
  `derive_autocalc_actor_values`
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md`, "NPC stat storage — NOTE
  (distinct from FO4)": *"Auto-calc-OFF NPCs store explicit skill/SPECIAL values in
  their `NPC_` record (DNAM-era layout); auto-calc-ON NPCs are computed from class base
  attributes (the #1663 path)."* Flag identity from
  `docs/engine/charal-fo4-ruleset.md`, "Inheritance chain" item 4: *"**ACBS
  "Auto-calc stats"** flag (bit 4) — as in FO3/FNV."*
- **Description**: the deferral itself is correct and correctly declared — the code
  does not guess a formula, which is the right call. What is wrong is its stated scale.
  The docstring says *"Correct for the auto-calc **majority**; an approximation for
  hand-tuned actors"*, which reads as a long tail. Measured against the ACBS bit the
  FO4 capture document names, the auto-calc set is a bare majority and the
  "hand-tuned" set is ~40 % of every actor in both games. A reader sizing the gap from
  the comment will under-weight it by an order of magnitude.
- **Evidence**: probe over vanilla masters, counting `acbs_flags & 0x0010`:

  | | FNV | FO3 |
  |---|---|---|
  | NPC_ records | 3816 | 1647 |
  | auto-calc **ON** (`0x0010` set) | 2283 (59.8 %) | 935 (56.8 %) |
  | auto-calc **OFF** | **1533 (40.2 %)** | **712 (43.2 %)** |

  `derive_autocalc_actor_values` never reads `acbs_flags`; it goes straight to
  `index.classes.get(&npc.class_form_id)` for every FNV/FO3 actor. The stored values it
  should prefer are not merely unread — they are **unparsed**: the only `b"DNAM"` arm
  that captures actor values (`parse_npc_actor_values`) is gated on
  `GameKind::uses_actor_value_properties`, i.e. FO4+.
- **Impact**: documentation accuracy, not behaviour — the behaviour cannot improve until
  the parse lands. But the gap is load-bearing for milestone planning: ~1500 FNV and
  ~700 FO3 actors, including most hand-authored named NPCs (exactly the ones quests and
  dialogue conditions target), carry class-averaged stats instead of their authored
  ones.
- **Related**: the enabling parse gap (FO3/FNV NPC_ DNAM skill/SPECIAL block) routes to
  `/audit-esm` Dimension 4 — it is NPC_ record parsing, not CHARAL. `CHAR-D5-02`
  compounds it (a templated actor can be wrong on both axes at once).
- **Suggested Fix**: correct the docstring to state the measured split and name
  `acbs_flags` bit 4 as the discriminator, and add the `/audit-esm` cross-reference for
  the blocking parse work. Once the DNAM skill block is parsed,
  `derive_npc_actor_values` gains a third arm gated on that bit — the resolve-or-skip
  shape it already uses elsewhere.

---

## Completeness Checks
- [ ] **SIBLING**: The same drift class is swept across the other capture documents / docstrings, not just the row cited
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
