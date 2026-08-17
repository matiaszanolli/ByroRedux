# FNV-2026-08-16-D6-01: every female and child FNV NPC spawns with the male body mesh

**Issue**: #3037
**Severity**: HIGH
**Dimension**: 6 — Animation/Skinning
**Labels**: `high,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_FNV_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_FNV_2026-08-16.md` (Dimension 6 — Animation, Skinning & Particles).

**Location**: `byroredux/src/npc_spawn.rs`:272-352 (`humanoid_body_paths`)

## Description

`humanoid_body_paths(game: GameKind)` takes **no gender parameter** and returns one hardcoded triple for Oblivion/FO3/FNV: `upperbody.nif`, `lefthand.nif`, `righthand.nif`.

Its doc comment justifies this with:

> "Female humanoids on FNV vanilla re-use the male body (verified 2026-04-28 — `_female\` directory not present in vanilla Fallout - Meshes.bsa). … See TD8-018 / #1117 for the placeholder-arg removal rationale."

**The observation is correct and the inference is not.** FO3/FNV place **both** genders in the same `_male\` directory, distinguished by a *filename prefix* — so the absence of a `_female\` directory says nothing at all. The gender argument was removed under #1117 on the strength of that bad inference.

## Evidence

Complete listing of `meshes\characters\_male\*.nif` in vanilla `Fallout - Meshes.bsa`, excluding `idleanims\`/`locomotion\`:

```
childfemaleupperbody.nif   childupperbody.nif
femaleupperbody.nif        femalelefthand.nif      femalerighthand.nif
femalelefthand1st.nif      femalerighthand1st.nif
femalelefthandpipboyglove.nif  femalelefthandpipboyglove1st.nif
upperbody.nif              lefthand.nif            righthand.nif
lefthand1st.nif            righthand1st.nif        skeleton.nif   …
```

**Independently re-verified 2026-08-17** by scanning the shipped `Fallout - Meshes.bsa` name block directly: `femaleupperbody.nif`, `femalelefthand.nif`, `femalerighthand.nif`, `childupperbody.nif` and `childfemaleupperbody.nif` are all **present**, and there is no `characters\_female` directory — confirming both halves of the analysis.

The gender bit is already decoded and already reaches this call site: `crates/plugin/src/equip.rs`:49-63 defines `Gender::from_acbs_flags` (ACBS bit 0), and `byroredux/src/npc_spawn.rs`:37 imports it and passes it at :688 into `resolve_armor_mesh`. **Only the body-path selector ignores it.**

## Impact

`FalloutNV.esm` carries 3,816 `NPC_` records. Every female one — Sunny Smiles, Trudy, Cass, the Legion/NCR female troopers, every female generic — renders with the male torso and male hands, and every child renders adult-sized.

Wrong silhouette, wrong skin weights against the shared skeleton, and armour authored to layer over the female body sits on the male one. **No workaround.**

## Suggested Fix

Restore a gender/age parameter to `humanoid_body_paths` and select the `female*` / `child*` / `childfemale*` prefixed meshes accordingly. `Gender::from_acbs_flags` is already in scope at the call site, so the plumbing exists — this reverses #1117's removal on corrected evidence.

**Update the doc comment**: its factual observation should be kept, its inference removed, so the same conclusion is not re-derived.

## Related

- #1117 / TD8-018 (the placeholder-arg removal this reverses)
- #793 (hands) — the only aspect prior audits checked
- **Eight earlier reports marked this area clean** on the strength of the same bad inference (`AUDIT_FNV_2026-05-30.md`:299, `AUDIT_FNV_2026-06-18.md`:144, `AUDIT_FO3_2026-06-14.md`:369, `AUDIT_FO3_2026-06-23.md`:95, `AUDIT_FO3_2026-07-05.md`:196, `AUDIT_FO3_2026-07-16.md`:235, `AUDIT_FO3_2026-08-03.md`:293, `AUDIT_FO3_2026-08-07.md`:317)

## Completeness Checks
- [ ] **SIBLING**: FO3 and Oblivion body-path selection fixed too — the same function serves all three
- [ ] **AGE**: Child meshes (`childupperbody` / `childfemaleupperbody`) selected as well as gender
- [ ] **DOC-COMMENT**: The false inference removed so it cannot be re-derived by a future audit
- [ ] **SKELETON**: Female/child bodies verified to skin correctly against the shared skeleton
- [ ] **TESTS**: A regression test spawns a known female FNV NPC and asserts the female mesh path

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3037 --json state` when live state is needed.*
