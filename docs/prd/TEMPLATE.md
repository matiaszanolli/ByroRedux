# <Feature name> — Product Requirements

<!--
Copy this file to docs/prd/<slug>.md (kebab-case, no "-prd" suffix), add a
row for it in docs/engine/index.md, replace every placeholder, and delete
these comments as you go. `python3 scripts/check-prd-conformance.py`
fails on any placeholder it finds (XXX-, YYYY-MM-DD, <slug>, <Feature name>).
The rules behind every section are in README.md; the worked example is
legacy-mod-compatibility.md.
-->

**Status**: PROPOSED (YYYY-MM-DD). <One sentence: which revision this is
and what it folds in. Statements about how an original engine, format or
tool behaves cite external sources; a claim that could not be verified is
marked **Phase 0: verify** instead of being stated as fact.>

<!-- Add when the PRD becomes ACTIVE; the gate requires it from then on:
**Tracking**: #NNNN (parent), #NNNN (Phase 0), …
-->

**Scope**: <What the feature promises, to whom, and what is explicitly
outside it. One paragraph.>

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| <mechanism> | [<Design>](../engine/<design>.md) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `XXX-` prefix.

---

## 1. Product thesis

<!--
Why the feature exists, in the owner's words where they exist. Who it is
for. End with the one corollary that shapes every requirement below (the
legacy-mod PRD's is "compatibility is promised per mechanism, never as a
global flag"). A reader should be able to say what the engine will NOT do
after this section.
-->

---

## 2. Terminology

<!-- Every term the requirements lean on, defined once, in a table. Link
to a related design's terminology instead of redefining its terms. -->

| Term | Meaning |
|---|---|
| **<Term>** | <Meaning> |

---

## 3. Core model

<!--
The conceptual model the requirements are phrased in: a closed vocabulary
of classes, tiers, states or dispositions, each with a meaning and how it is
reported to the user. If the model has an inventory (mechanisms, record
types, passes, formats), put it here as a table with a "Today" column that
states the behaviour at HEAD; the gate requires that column to be dated
("at HEAD on YYYY-MM-DD"). A hand-written "Today" column goes stale: say
what replaces it (a generated catalog, a test) and when.
-->

The inventory below is the state at HEAD on YYYY-MM-DD.

| # | <Item> | <Class> | Today |
|---|---|---|---|
| 1 | <item> | <class> | <state at HEAD, with the file or issue that proves it> |

---

## 4. Goals and non-goals

### 4.1 Goals

<!-- Each goal is one measurable outcome, phrased so §8 can count it. -->

- **XXX-G-001 — <Name>.** <Outcome.>

### 4.2 Non-goals

<!-- What is deliberately out, with the reason or the design that already
decided it. A non-goal that is merely "later" belongs in §9 as a phase. -->

- **XXX-NG-001.** <Exclusion and why.>

---

## 5. Requirements by area

<!--
One H3 per area, each with its own ID segment (`XXX-AREA-NNN`). One
observable per ID: a command, a value, a report line, a number. State at
HEAD goes in the sentence ("Today the resolver tie-breaks by UUID; it MUST
…"). Unverified behaviour of an original engine is marked **Phase 0:
verify**. Never reuse a number once the PRD has been reviewed; retire an ID
with "(withdrawn, rev N)" and keep the slot.
-->

### 5.1 <Area> (`XXX-AREA`)

- **XXX-AREA-001.** <Subject> MUST <observable>. Today: <state at HEAD>.
- **XXX-AREA-002.** <Subject> SHOULD <observable>.

---

## 6. Deliberate deviations and risks

<!--
Every place the engine knowingly departs from original-engine behaviour,
with what that makes redundant and the risk the departure creates for
users or content, pointing at the requirement that handles the risk. This
is the register a reviewer reads to find silent breakage.
-->

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | <legacy limit, cited> | <ours, requirement id> | <tool / mod class> | <risk → XXX-AREA-NNN, or "None" with why> |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

<!-- All seven titles, each in / out / not applicable with a reason. Follow
the engine tiers in docs/engine/game-compatibility.md. If delivery is in
waves, say which titles form each wave and why. -->

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | <yes / no / n/a> | |
| Fallout 3 | | |
| Fallout: New Vegas | | |
| Skyrim (LE / SE / AE) | | |
| Fallout 4 | | |
| Fallout 76 | | |
| Starfield | | |

### 7.2 Acceptance

<!-- Scenarios a tool can run: `cargo test` names, `--bench-hold` +
`byro-dbg` scripts under docs/smoke-tests/, or a corpus with a pass
definition. Name the data each scenario needs; mod or game files are never
checked in. -->

A scenario passes when:

1. <Observable condition.>
2. <Observable condition.>

---

## 8. Metrics

<!-- Definitions and granularity only. The numbers live in ROADMAP.md (one
home); say which table there. -->

| Metric | Definition | Granularity |
|---|---|---|
| <name> | <numerator ÷ denominator> | <per game / per phase / …> |

---

## 9. Delivery sequence

<!--
Phases ordered by what they unblock. Phase 0 is survey, catalog and
decisions, with no engine code. Every phase ends with "Exit:" followed by a
measurement, not a feature list; the gate requires the word. Name the
milestones each phase depends on.
-->

**Phase 0 — <Name> (no engine code).** <What is surveyed, cataloged,
verified; which §10 decisions it needs resolved; which **Phase 0: verify**
items it closes.> Exit: <measurable condition>.

**Phase 1 — <Name>.** <What ships; the requirement IDs it satisfies.>
Exit: <measurable condition>.

---

## 10. Decisions required

<!--
Each needs the owner. The recommendation is the first option; name at
least one alternative and the consequence of each. When a decision is
taken, append `**Decided** (YYYY-MM-DD): <answer>` to its item and fold the
answer into the requirements; the gate refuses APPROVED while any item
lacks it. If there are none, write "No decisions required."
-->

1. **Q1 — <Question>.** Recommended: <option, why>. Alternative: <option,
   consequence>.

---

## 11. Document map

| Need | Go to |
|---|---|
| <need> | [<doc>](../engine/<design>.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

<!-- One row per revision and per state change. The review cell is `—` or
`<who>: revise | approved | withdrawn`; only the owner's `approved` moves
the status. The last row's date and the status line's date are the same
date. -->

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | YYYY-MM-DD | First draft | — |
