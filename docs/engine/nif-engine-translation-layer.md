# BASE ISSUE — Complete the NIF→Engine translation layer

**Status**: OPEN / base (umbrella) issue — opened 2026-05-27 — tracked as [#1277](https://github.com/matiaszanolli/ByroRedux/issues/1277)
**Driving symptom**: FNV/FO3/FO4 interiors are broken at the **geometric** level
(not just material), while Skyrim interiors render correctly. See Exhibit A.
**Parent of**: [`material-abstraction.md`](./material-abstraction.md) (material axis,
already in progress) + the new geometry/transform axis defined below.

---

## 1. Thesis

ByroRedux already states the right invariant — *translate every game's native
data into one canonical representation at the parser→engine boundary; the
renderer/shader stays game-agnostic* (`feedback_format_translation.md`,
`format_abstraction.md`). That invariant is **realised for the shader** (verified:
zero `if (game == …)` branches in `triangle.frag`) but **not for the translation
layer that feeds it**. The canonical `Material` *and* the canonical
`Transform`/geometry are populated by per-game paths that converge for Skyrim and
diverge for the Fallout line.

The result is the "different stages of development" look: Skyrim is gorgeous,
Fallout is broken — same engine, same shader, **different translation fidelity**.

## 2. Exhibit A — FNV casino interior (geometric breakage)

A FNV casino interior (Atomic Wrangler / Gomorrah class) renders with:

1. **A grossly oversized / mis-proportioned cylindrical wall element** dominating
   the scene — a modular architecture piece whose proportions are wrong, while the
   railings, stools, tables and the NPC around it are correctly placed and scaled.
   This is a *geometry/transform* defect, not a material defect.
2. A **hard-edged directional light shaft on the interior floor** with a crisp
   shadow boundary — consistent with the M34 default exterior sun leaking into an
   interior cell that should be lit only by its interior lights + ambient.
3. A **posterized / black-banded fixture** (the gold star lamp) — the
   self-illumination / fallback class tracked separately.

The contrast is the signal: Skyrim's `WhiterunDragonsreach` (5 885 entities) is
clean; FNV/FO3 interiors are not. Same renderer.

## 3. Why this is a translation-layer problem (not a shader problem)

The shader branches only on game-agnostic `mat.materialFlags` / `mat.materialKind`
/ `dalcFlags`. If two games look like different engines, the divergence entered
*upstream*, at translation. There are two axes:

### Axis 1 — Material (IN PROGRESS)

Fully scoped in [`material-abstraction.md`](./material-abstraction.md). The
smoking gun there: FNV's `classify_pbr_keyword` path collapses **every** surface
to `metalness 0.00 / roughness 0.80` (metal → matte plastic, glass → rough
plastic), while FO4 BGSM gives real per-material PBR (`metal 0.79/0.04`). Same
shader, two conventions → "different dev stages." Steps 1–3 (ground-truth audit,
canonical PBR at parse, parse-time glass) have landed; emissive-scale and ambient
unification remain.

### Axis 2 — Geometry / Transform (NEW — initial hypothesis FALSIFIED 2026-05-27)

**Original hypothesis (DISPROVEN):** that `import/coord.rs::zup_matrix_to_yup_quat`
→ `svd_repair_to_quat` discards non-uniform scale/shear baked into `NiTriShape`
3×3 matrices, ballooning Fallout modular architecture.

**Measurement (`crates/nif/examples/dump_transforms.rs`, this session):** scanned
every AV-bearing block in FNV architecture, computing each rotation matrix's
column norms and column dot-products.

| Corpus | NIFs | AV blocks | non-identity rot | non-uniform scale | shear | max col-norm spread |
|---|---:|---:|---:|---:|---:|---:|
| `architecture/` (all) | 2034 | 10 837 | — | **0** | **0** | — |
| `architecture/strip/` (casinos) | 263 | 2 089 | 427 (20%) | **0** | **0** | **0.00000** |

The tool is proven live (20% of casino matrices are genuine non-identity
rotations) yet the column-norm spread and off-diagonal are **exactly zero**: FNV
matrices are perfectly orthonormal. The only scaling is the scalar
`NiTransform.scale` (observed range 0.25–2.0), which the uniform-scale model
carries correctly. **The transform/coord translation loses no geometric
information for FNV.** The SVD repair is a no-op on this content.

**Conclusion:** the Exhibit-A "broken geometry" look is **not** a transform-fidelity
bug. The remaining candidates, in order of likelihood:

1. **Material collapse masquerading as geometry (Axis 1)** — the FNV keyword path
   flattens every surface to `metalness 0 / roughness 0.8`, so a large curved
   column reads as a featureless matte-brown mass. This is the confirmed divergence
   and the most probable driver of the "undetailed blob" perception.
2. **Interior lighting leak (Axis C)** — the hard-edged floor sun-shaft is the M34
   default exterior sun active in an interior cell.
3. **Residual per-REFR placement** (position/rotation, *not* scale) — cannot be
   ruled out without identifying the specific REFR interactively (`pick`/`mesh.info`).
   If a true-geometry defect survives A+C fixes, this is where to look — but there
   is no systemic translation loss feeding it.

## 4. Meta-cause — why the audits never caught this

This is the answer to the original "audits are outdated and ignore plain-sight
issues" complaint. The `audit-*` suite inspects **code correctness** (unsafe
blocks, lock ordering, stream position, stale paths, struct-layout pins). **No
audit dimension inspects rendered output or per-game translation *fidelity*.** The
genuinely impactful Fallout bugs (NPCs as floating equipment, chrome materials,
fall-through floors, this geometric breakage) were all found by a **manual
headless telemetry sweep** (`FALLOUT_SYMPTOMS_2026-05-26.md`), never by an audit.
Two structural gaps:

- **No runtime/visual audit dimension** — nothing drives the engine headless and
  diffs telemetry (`tex.missing`, `mesh.info`, bounds, parse-fail rate) against a
  known-good baseline per game.
- **No translation-completeness audit** — nothing asserts that the canonical
  `Material`/`Transform` produced for equivalent surfaces is convention-identical
  across games. The per-game `audit-fnv`/`audit-fo3`/`audit-fo4` skills check
  *parse rate*, not *post-translation equivalence*.

Even the static gaps the audits *should* have caught (F3 `bhkNPCollisionObject`
dropped by `extract_collision`; F4 game-unaware `BSXFlags` bit-5 gate) sat in
exactly the per-game NIF-import/cell-loader translation logic the audits underweight.

## 5. Scope & child workstreams

| # | Workstream | State | Tracking |
|---|---|---|---|
| A | Canonical **material** convergence | in progress | `material-abstraction.md` steps 4–5 |
| B | Canonical **geometry/transform** — preserve non-uniform scale/shear | **NEW** | this issue, §6 |
| C | Interior vs exterior **lighting** translation (sun-shaft leak) | NEW | this issue |
| D | **Runtime/visual audit** dimension (headless telemetry diff) | NEW | extends `audit-*` |
| E | **Translation-completeness audit** (cross-game canonical equivalence) | NEW | extends `audit-fnv/fo3/fo4` |

A is already moving. B is the new geometric core. D+E are the meta-fix so this
class stops being invisible.

## 6. Diagnostic plan

- [x] **Inspect source NIF node matrices** — `dump_transforms.rs` built; scanned
  all FNV architecture (10 837 AV blocks). Result: **0 non-uniform / 0 shear**. The
  transform-loss mechanism does not exist for FNV content (§3 Axis 2 table).
- [ ] **Identify the actual broken mesh interactively** — load the casino cell with
  `--bench-hold`, attach `byro-dbg`, `pick`/`mesh.info` the oversized element,
  capture its REFR form id + NIF path + authored `bound`/extents + applied
  `final_scale`. This requires the cell name (the screenshot reads as Atomic
  Wrangler / a Strip casino) and a Vulkan device — out of `cargo test` scope.
- [ ] **Re-test after Axis A + C land** — fix the material collapse (A) and the
  interior sun leak (C), then re-screenshot. If the column still reads as
  geometrically wrong, escalate to per-REFR placement; otherwise the geometric
  perception was material/lighting all along.

_Step removed:_ "decide a `Vec3`-scale canonical representation" — moot, since no
content carries non-uniform scale.

## 7. Acceptance criteria

- The FNV casino interior in Exhibit A renders with correct geometry proportions.
- An automated check asserts the canonical `Transform`/`Material` for equivalent
  surfaces is convention-identical across FNV/FO3/FO4/Skyrim (workstream E).
- A headless visual/telemetry audit dimension exists and is runnable per game (D).
- No new `if (game == …)` branch enters the shader or renderer (invariant held).
