---
description: "Scaffold a feature PRD from the template, or review one against the PRD standard (automatic gate + manual rubric) and propose its next state"
argument-hint: "new <slug> \"<Feature name>\" <PREFIX> | review <slug> | list"
---

# PRD — scaffold, review, list

The standard is `docs/prd/README.md`; read its §1 (triggers), §3
(lifecycle), §5 (writing rules) and §8 (rubric) before either mode. The
template is `docs/prd/TEMPLATE.md`; the worked example is
`docs/prd/legacy-mod-compatibility.md`. The automatic half of the gate is
`scripts/check-prd-conformance.py` (codes explained in README §7).

Parse `$ARGUMENTS`: the first word is the mode.

---

## `list`

```bash
python3 scripts/check-prd-conformance.py --list
```

Print the table as returned. Nothing else.

---

## `new <slug> "<Feature name>" <PREFIX>`

1. **Check the trigger.** Ask yourself which README §1 trigger (T1–T5) the
   feature meets and say so in one line. If none applies, say that a PRD
   is not required and stop — do not scaffold a PRD for a bug fix or a
   single-issue change.
2. **Validate the arguments.** `<slug>` is kebab-case and
   `docs/prd/<slug>.md` does not exist. `<PREFIX>` is 2–6 capitals and is
   free: `grep -rhoE '^\s*[-|*]\s*\*\*<PREFIX>-' docs/prd docs/engine`
   must return nothing. If it collides, propose another and stop.
3. **Scaffold.** Copy `docs/prd/TEMPLATE.md` to `docs/prd/<slug>.md` and
   replace, with `sed`, `<Feature name>` → the name, `YYYY-MM-DD` → today,
   `XXX-` → `<PREFIX>-`, `<slug>` → the slug. Leave the HTML guidance
   comments in place; the author deletes them section by section, and the
   gate ignores them.
4. **Register.** Add a row to the **Subsystems** table in
   `docs/engine/index.md`:
   `| [<Feature name> — PRD](../prd/<slug>.md) | <crates> | PROPOSED (<today>). <one-line summary> |`
   Place it next to the designs it will relate to.
5. **Run the gate** on the new file:
   `python3 scripts/check-prd-conformance.py docs/prd/<slug>.md`.
   Expected at this point: E11 for the placeholder design links and
   nothing else. Any other error means the template or the scaffold step
   drifted; fix the cause, not the symptom.
6. **Hand over.** Tell the user the path, the prefix, the index row, and
   the three things the author fills first: §1 thesis (the owner's
   framing, verbatim where it exists), §3 core model, §10 decisions. Do
   not write the PRD's content unless asked; when asked, apply README §5
   R1–R10 and mark every unverifiable original-engine claim
   **Phase 0: verify**.

---

## `review <slug>`

A review round. Output is a findings list the author can fold in, plus a
proposed log row. **Never flip the status yourself**: `approved` is the
owner's verdict; this mode may only recommend `revise` or "ready for the
owner's approval".

### Step 1 — Automatic gate, strict

```bash
python3 scripts/check-prd-conformance.py docs/prd/<slug>.md --strict
```

Every error is a finding as-is. Every warning becomes a finding unless the
author can defend it (W01 on a declarative sentence that is clearly a
requirement is defensible; W02 never is).

### Step 2 — Manual rubric M1–M15 (README §8)

Walk the fifteen items in order. For each, write `Mn: pass` or `Mn /
<ID or §>: <finding>`. Two items require work outside the document:

- **M6 — HEAD is true.** Pick at least three "Today" / "shipped" /
  "not implemented" claims, spread across sections, and verify each
  against the code at HEAD (`rg` the symbol, read the function, run the
  named test if it is cheap). Quote the file:line that proves or refutes
  each one. One wrong claim fails the round. This is the audit-hygiene
  premise rule applied to PRDs: never fix a document around a premise you
  have not re-measured.
- **M5 — Nothing guessed.** For every claim about how an *original*
  engine, format, tool or mod ecosystem behaves, check that it cites an
  external source. Acceptable: UESP and the CK / GECK / Fallout CK wikis
  (the local dumps under `/mnt/data/src/reference/*-uesp-wiki/` count),
  xEdit / LOOT / Wrye Bash docs and source, extender sources, format
  specs. Not acceptable: this repository, its issues, its memories, or
  "commonly known". An uncited claim is a finding that proposes the exact
  **Phase 0: verify** wording to replace it.

Also check, explicitly:

- **M13** against CLAUDE.md's six architecture invariants, ROADMAP's keep
  list and anti-scope list. Quote the invariant for any conflict.
- **M12**: every decision is the owner's to make (a question the author
  could settle by reading code is not a decision; it is a verify item).
- **M14**: anything restated from ROADMAP's matrix, the fix catalog or a
  design's requirement list is a finding ("link, don't copy").

### Step 3 — Report

Return, in this order:

1. The gate's one-line verdict and the strict output, verbatim.
2. The rubric table: fifteen rows, `pass` or the finding.
3. **Findings**, numbered, each `Mn / <ID or §>: <what is wrong> → <what
   to write instead>`. Most severe first: wrong HEAD claims, uncited
   original-engine claims, untestable requirements, invariant conflicts,
   then everything else.
4. **Verdict**: `revise` with the count of findings, or "ready for the
   owner's approval" (zero findings, zero gate errors, every §10 item has
   a recommendation and the owner still has to answer them).
5. The log row to append, ready to paste:
   `| <next rev> | <today> | Review round <n> folded in: <topics> | <reviewer>: revise |`
   where `<reviewer>` is `claude` for this pass. The author adds it when
   folding the findings in, and bumps the status-line date to match.

Do not edit the PRD in review mode. If the user then asks you to fold the
findings in, do that as a separate step: one revision, one log row, the
status-line date updated, then re-run Step 1 and confirm zero errors.
