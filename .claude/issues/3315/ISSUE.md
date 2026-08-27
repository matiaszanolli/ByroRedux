# FNV-D2-01

**Issue**: #3315
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: HIGH
**Dimension**: 2 — NIFAL Canonical Translation
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)

> **Severity escalated MEDIUM → HIGH at publish time** per `.claude/commands/_audit-severity.md`'s special-rules row *"Wrong/divergent `Material` out of NIFAL `translate_material`" → HIGH minimum*.


**File**: `crates/core/src/ecs/components/material.rs:807`

**Premise verified**: the arm is live at HEAD and matches on the *whole path*
with an unbounded substring test:

```rust
// crates/core/src/ecs/components/material.rs:807
if contains_any_ci(path, &["skin", "body", "head", "hand", "face"]) {
    return PbrMaterial { roughness: 0.5, metalness: 0.0 };
}
```

`contains_any_ci` (as opposed to the sibling `contains_any_ci_word` at
`material.rs:1103`, which the same file already uses for `ice` / `gem` / `fur`
after #2009 / MAT-D1-01) has no word-boundary guard. Bethesda's FNV weapon
texture tree is `textures\weapons\1handpistol\…`, `…\2handrifle\…`,
`…\1handmelee\…`, `…\2handautomatic\…` — every one of which contains `hand`.
The arm sits *after* metal/precious/glass/wood/stone/cloth, so it only fires
when nothing else matched, which is the normal case for a gun texture.

**Evidence** (full FNV + all 5 DLC BSAs, live `import_nif_scene`):

```
weapon-dir skin-classified meshes: 3458  (457 distinct texture paths)
material_kind: {0: 3359, 102: 99}          # 3,359 are ordinary LIT surfaces
classification without the 'hand' collision:
    {"r0.85/m0.00": 1802, "r0.80/m0.00": 1211, "r0.50/m0.00": 440, "r0.35/m0.00": 5}
```

Top offenders by mesh count (skin arm, `Fallout - Meshes.bsa`):

```
  88 textures\weapons\1handpistol\10mmpistol.dds
  74 textures\gore\handymeatcap.dds
  50 textures\weapons\2handrifle\laserrifle01.dds
  41 textures\creatures\misterhandy\mrhandy01.dds      # a ROBOT
  40 textures\weapons\2handautomatic\battlerifle.dds
  40 textures\weapons\2handrifle\varmint_22_d.dds
  37 textures\weapons\1handmelee\ripper.dds
```

Whole-corpus skin-arm breakdown (4,477 meshes total):

```
weapon-dir(1hand/2hand/\weapons\) 3458 | plausible-skin 386 | *head compound 225
misterhandy robot 141 | interface 116 | other 151
```

→ **~91 % of every SKIN classification on the reference title is a false
positive**, and the dominant class is "every gun in the game".

The value survives to the GPU. `translate_material` seeds
`metalness/roughness` from the import-time overrides
(`material_translate.rs:504-505`), `resolve_pbr` only clamps, and the one
spawn-time post-mutator, `resolve_normal_alpha_spec_roughness`
(`material_translate.rs:759`), returns `None` when the bound normal map carries
alpha (`normal_alpha_spec_roughness`, `material_translate.rs:737`). FNV weapon
normals are alpha-bearing — sampled from `Fallout - Textures.bsa`:

```
10mmpistol_n.dds  DXT5   laserrifle01_n.dds DXT3   battlerifle_n.dds DXT5
ripper_n.dds      DXT5   varmint_22_n.dds   DXT5
```

so the 0.50 is what `render/static_meshes.rs:342` reads and forwards.

**Impact**: 3,458 FNV meshes (5.5 % of the corpus), of which 3,359 are lit
geometry, shade with a skin-tier GGX lobe (roughness 0.50, α = 0.25) instead
of the 0.80–0.85 they would otherwise earn (α = 0.64–0.72). That is a visibly
tighter, glossier direct-light highlight on the single asset class that is on
screen 100 % of the time in a first-person game — pistols, rifles, laser
weapons, melee weapons — plus Mister Handy robots and gore caps. Both
classifications are dielectric (metalness 0.0), so this does **not** open the
`triangle.frag:2536` RT env-reflection gate (`metalness > 0.3 ||
hasExplicitEnvironment`); the severity ceiling is set by that. Caveat in the
honest direction: my counterfactual ran the classifier with
`specular_authored: false`, so for the 1,211 meshes that would land on the
env-map arm (`material.rs:846-874`) the real delta may additionally include a
metalness lift up to 0.4 — which *would* cross the RT gate. It is a
no-fabrication violation in the checklist's own terms: translation is
asserting "this 10mm pistol is skin", a semantic the source never authored.

**Fix sketch**: move `hand`/`head`/`face`/`body` off `contains_any_ci` — either
to `contains_any_ci_word` (fixes `interface`, `defaced`, `bobblehead`; does
**not** fix `1handpistol`, which has no separator), or better, follow the arm's
own existing precedent and scope the skin tokens to a character/creature asset
family the way the metal arm scopes `dwemer`/`dwarven` to `filename`
(`material.rs:727-731`) — e.g. require the path to be under
`\characters\` / `\creatures\` / `gore`. Pin with a regression test over the
real FNV weapon paths above.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
