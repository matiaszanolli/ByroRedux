# Batch: #2705 #2706 #2707 #2708

## #2705 — SF-2026-08-12-D3-01: The 105 MB `materialsbeta.cdb` is fully decompressed and immediately discarded on every `build_material_provider` call

**Severity**: MEDIUM · **Labels**: bug, import-pipeline, medium, performance
**Location**: `byroredux/src/asset_provider/material.rs:44-52` (`discover_starfield_cdbs`), `:352-384` (`register_starfield_cdb`)

`discover_starfield_cdbs` calls `archive.extract(&path)` for every discovered
CDB, which for a BA2 GNRL entry runs the full zlib inflate into an owned
`Vec<u8>`. `register_starfield_cdb` then reads exactly the 4-byte magic and
the 12-byte header (`probe_header`), bumps a counter, and the `Vec` is
dropped at the end of the loop iteration. Nothing retains the bytes. Phase 1
needs 16 bytes; it pays 105 MB of inflate + allocation for them, per CDB, per
provider build.

Measured — `materials\materialsbeta.cdb` extracts to 105,037,616 bytes from
the 17.6 MB `Starfield - Materials.ba2`. `MaterialProvider` has no field
holding CDB bytes (only `sf_cdb_count: usize`, `material.rs:277`).
`build_material_provider` runs fresh at boot, at every door/cell transition,
at save-load, and at debug-load — the same call-site set #2615 was filed
against.

Impact: ~105 MB transient allocation + a multi-hundred-ms inflate on every
cell transition on Starfield, for a presence bit.

Related: #2615 (SF-D3-03, fixed sibling), #2039 / PERF-D7-02, #2359 (Phase 2
— which *will* need the bytes, so the fix should be a cache, not a narrower
read).

Suggested Fix: Either (a) add a bounded `Vec<u8>`/`Arc<[u8]>` hold keyed by
archive+path so the Phase-2 parse and cross-cell rebuilds reuse it — the
shape `csg_cache` next to it already uses — or (b) short-circuit discovery
when the same (archive path, CDB path) pair was already registered this
session.

Source: `docs/audits/AUDIT_STARFIELD_2026-08-12.md` (finding `SF-D3-01`)

---

## #2706 — SF-2026-08-12-D3-02: Three doc comments cite a `MaterialProvider::sf_cdbs` `Arc` cache that does not exist, and the claim actively contradicts the code

**Severity**: LOW · **Labels**: documentation, low
**Location**: `byroredux/src/asset_provider/material.rs:280`, `:311`, `byroredux/src/app_step.rs:450`

`csg_cache`'s field doc says it "mirrors the `sf_cdbs` `Arc` hold";
`geometry_csg`'s doc repeats "mirrors the `sf_cdbs` `Arc` caching`;
`app_step.rs`'s caching design note lists "`MaterialProvider::sf_cdbs`"
among the caches discarded on rebuild. `grep -rn sf_cdbs byroredux/src/`
returns only those three doc hits — the field was replaced by
`sf_cdb_count: usize` and no CDB bytes are cached anywhere.

Impact: Documentation-only, but the false claim is load-bearing in the
wrong direction: a reader auditing provider-rebuild cost would conclude the
CDB is already `Arc`-cached and stop, which is exactly how
SF-2026-08-12-D3-01 stayed unnoticed.

Suggested Fix: Reword all three to reference the real `csg_cache`
precedent, or land the cache and make the comments true.

Source: `docs/audits/AUDIT_STARFIELD_2026-08-12.md` (finding `SF-D3-02`)

---

## #2707 — SF-2026-08-12-D8-01: `classify_legacy_pbr` stamps a fabricated `Some(0.0)/Some(0.85)` PBR pair onto 97.9% of Starfield meshes from an input set that is empty by construction, permanently disabling the NaN-sentinel fallback

**Severity**: MEDIUM · **Labels**: bug, nif-parser, medium, legacy-compat
**Location**: `crates/nif/src/import/material/mod.rs:1194-1218` (`classify_legacy_pbr`), `:1269-1270` (the unconditional `Some(...)` write), `crates/core/src/ecs/components/material.rs:816-842` (`resolve_pbr`), `byroredux/src/asset_provider/material.rs:726-739` (the `.mat` early return)

Status: NEW — distinct from #2359, which is about the *merge* forwarding
nothing; this is about the *importer* asserting a resolved value it did not
derive from anything.

On a Starfield material-reference stub the walker returns at
`dedicated_shader.rs:86` before writing a single `MaterialInfo` field, so
`into_imported_material` calls `classify_legacy_pbr` on an all-defaults
`MaterialInfo`: `texture_path = None` → `path = ""` (no keyword can match),
`specular_authored = false`, `has_normal_map = false`, `has_gloss_map =
false`, `env_map_scale = 0.0` (the `MaterialInfo::default`, `mod.rs:1061`,
which fails the `> 0.3` arm). Every classifier arm falls through to the
terminal `PbrMaterial { roughness: 0.85, metalness: 0.0 }`
(`material.rs:757-759`). That constant is then written as
`metalness_override: Some(0.0)`, `roughness_override: Some(0.85)` —
indistinguishable downstream from an authored value.

Evidence: 38,120 of 38,930 sampled Starfield meshes (97.9%) take this exact
path. Because both overrides are `Some`, `translate_material` never seeds
the NaN sentinel, so `Material::resolve_pbr`'s backstop (`material.rs:817`)
is unreachable for Starfield.

Impact: (a) Today: a single invented matte-dielectric constant on
essentially all Starfield content, presented to the Disney BSDF lobe as
resolved data. (b) After #2359 Phase 2 lands: any `.mat` the CDB index
*misses* will silently keep the fabricated `0.0/0.85` instead of falling
back to the sentinel. This is the NIFAL no-fabrication rule
(`docs/engine/nifal.md`) applied at the boundary.

Related: #2359, #2353, #2330.

Suggested Fix: When `MaterialInfo` carries no authored signal at all (the
stub-guard case), leave `metalness_override`/`roughness_override` as `None`
so `translate_material` seeds the NaN sentinel and `resolve_pbr` owns the
default — one code path for "unknown", instead of a fabricated `Some` that
outranks Phase 2's own miss-detection.

Source: `docs/audits/AUDIT_STARFIELD_2026-08-12.md` (finding `SF-D8-01`)

---

## #2708 — SF-2026-08-12-D9-02: The REFR-overlay material resolver is a second, parallel external-material path that knows only `.bgsm`/`.bgem`, so Starfield `.mat` overlays resolve to nothing even after CDB Phase 2 lands

**Severity**: LOW · **Labels**: bug, import-pipeline, low
**Location**: `byroredux/src/cell_loader/refr.rs:192-233` (`fill_from_bgsm`)

`fill_from_bgsm` dispatches on `path.ends_with(".bgsm")` / `".bgem")` and
returns silently for anything else. Its own doc says "No-op when the path
isn't a `.bgsm` / `.bgem`", so the omission is deliberate — but it means the
engine has **two** external-material resolvers with divergent format
coverage: `merge_external_material` (BGSM + BGEM + a `.mat` arm) and this
one (BGSM + BGEM only). A Starfield REFR whose XATO/MSWP supplies a `.mat`
path gets the path propagated into `ov.material_path` (and thence onto the
spawned material) but no role fills, and there is no place for a future CDB
lookup to hook in on this side.

Impact: Zero today — Starfield content resolves no textures from either
path (#2359), and `.mat` overlays on vanilla Starfield REFRs are rare. It
becomes a real, silent per-REFR divergence the moment #2359 Phase 2 lands
and the two resolvers disagree about what a `.mat` yields.

Related: #2359, #2594, SF-2026-08-12-D9-01.

Suggested Fix: Note the format gap in the doc comment now, and route both
resolvers through one shared "resolve external material → roles" helper
when Phase 2 lands, rather than adding a second `.mat` arm here.

Source: `docs/audits/AUDIT_STARFIELD_2026-08-12.md` (finding `SF-D9-02`)
