# TD4-2026-10-15-04: The `audit-scripting` Codex shim's description omits the M47.2 Papyrus VM and the `/audit-papyrus` split

Labels: low,tech-debt,documentation,doc-rot

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD4-2026-10-15-04`).

- **Severity**: LOW
- **Dimension**: 4 - Audit-Finding Rot
- **Location**: `.agents/skills/source-command-audit-scripting-skill/SKILL.md:3` (description) vs `.claude/commands/audit-scripting/SKILL.md:2`
- **Status**: NEW
- **Age**: shim last touched 2026-10-12 (`5d31fd637`) or earlier; the canonical description gained the VM clause and the compiler-frontends handoff since
- **Effort**: trivial
- **Description**: The shim body is a correct thin pointer and its "seven dimensions" is right, but its `description:` (the text Codex matches triggers on) lacks the M47.2 Papyrus VM (budget, reference/alias scripts, update/event registrations) and "compiler frontends are /audit-papyrus", and says "SKSE/JContainers provider-compat" where the canonical says extender-compat provider layer. A request for a "Papyrus VM audit" does not trigger it. A word-diff of all 34 shim/canonical descriptions shows every other shim within 0-5 minor words; this is the only material divergence. `audit-sync` tells the `meta` group to check shim descriptions; this one slipped.
- **Evidence**: shim vs canonical description diff (16 canonical words absent from the shim).
- **Impact**: wrong routing for non-Claude agents; low.
- **Related**: `audit-sync/SKILL.md:43-45`.
- **Suggested Fix**: copy the canonical description clause into the shim's `description:` and its "Use when" list.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
