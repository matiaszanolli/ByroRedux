# Bundle: #3855 #3856 #3857 #3858

Four Dimension-1 (file/function complexity) findings from
`AUDIT_TECH_DEBT_2026-09-05` at `fa5c4191`. All four premises re-verified at
HEAD — and all three files had grown *further* since the audit:

| File | As audited | At HEAD | Drift |
|---|---:|---:|---:|
| `byroredux/src/boot.rs` | 2670 | 3229 | +559 |
| `crates/nif/src/import/walk/mod.rs` | 2167 | 2278 | +111 |
| `byroredux/src/asset_provider/material.rs` | 2044 | 2219 | +175 |

## The finding that generalises: these files are read as *text*

The issues describe these as mechanical file splits. They are not, and the
reason is uniform across all three: **source-shape tests `include_str!` these
files**, because the invariants they pin (which types a system declares, what
order systems register in, which `Ni*Light` blocks reach the `LightKind`
boundary, whether a deferral marker is still present) are only observable as
source. Deleting the file breaks the build; the *dangerous* repair is
retargeting each test to whichever new file it now lives in, which narrows
every scan silently and turns a real assertion into a vacuous pass.

Counts found: **30** sites for `boot.rs`, **2** for `walk/mod.rs`, **3** for
`material.rs` — and the third `material.rs` reader
(`material_translate.rs:2933`) was missed by grep and only surfaced when the
compiler hit it.

**Rule for any future split in this repo:** run
`grep -rn 'include_str!("<file>")'` *before* moving anything.

The fix, where a whole-file scan was needed, is a `SOURCES` constant —
`concat!(include_str!(…), …)` over the production files, which every consumer
reads instead. `concat!` does expand nested `include_str!` (verified before
relying on it). Full detail in [INVESTIGATION.md](INVESTIGATION.md).

## #3855 — `boot.rs` → `boot/`

`8c5e02aa`. Ten files; every `pub(crate)` entry point re-exported so no call
site outside the directory changed. `boot::SOURCES` feeds all 30 assertions.

**Its order is load-bearing** and this is the subtle part: each source-shape
module truncates the string at its own `mod` declaration (so its own mentions
of a system name cannot be what a scan finds), which only holds while every
registration precedes the first test module. Natural `mod`-declaration order
puts `schedule/mod.rs`'s four test modules ahead of the five stage files and
truncates the whole schedule away — six tests failed exactly that way. Then
the pin written to catch it reintroduced the same failure from the other side,
by containing the sentinel as a literal at offset ~0 of the concatenation.

Two gates, both negative-tested:
`every_production_file_precedes_the_first_test_module` (per sentinel × per
file, each with a witness registration) and
`the_concat_list_covers_every_file_in_the_boot_directory`.

## #3856 — `walk/mod.rs` satellites

`9aae918b`. The three satellite walkers really are independent entry points
(called only from `import/mod.rs`), so they moved verbatim.

Two deviations from the issue's table, both forced by the code:
`resolve_affected_node_names` / `resolve_block_ref_names` stay in `mod.rs`
because `imported_light_from_base` needs them too, and `ParticleMaterial`'s
fields became `pub(super)` because `walk_node_flat` stayed behind and reads
them.

Gate: `the_scene_graph_walkers_never_call_a_satellite_walker`, pinning the
independence that made the extraction safe.

## #3857 — `material.rs` → `material/` + merge decomposition

`42f0ead4`. Four files. `merge_external_material` 989 → 215 LOC, with the two
arms as private siblings (544 / 247).

**#2412's invariant is preserved literally.** That issue closed with an
explicit *no-action* recommendation — the single NIFAL boundary "should not be
split in a way that weakens that invariant" — so `merge_external_material` is
still the one exported function in `merge.rs`, gated by
`merge_external_material_is_the_only_exported_fn_in_this_file`. The boundary
is a *visibility* claim, which is precisely what a behavioural test cannot
see: `merge_bgsm_arm` would keep passing every merge test on the day someone
marks it `pub(crate)`.

**The `MergeSentinels` struct the issue proposed is unnecessary.** It assumed
the ~16 `set_*` bools were shared across arms. They are not — every one is
written and read only inside the BGSM arm — so they moved in as plain locals.

`half_evict` replaces **six** copies of the bounded-LRU block (the issue
counted four), including the file's only nesting-depth-7 site.

Also fixed: `cargo fix` prunes re-exports that only `#[cfg(test)]` consumers
reach through a glob, breaking the test build while
`cargo check --all-targets` reports clean.

## #3858 — census entry, highest-value function decomposed

The issue explicitly says **no bulk action**; it exists so the next sweep can
diff the 133-function count and so nobody reads "the file was split" as "the
complexity was reduced". Per the user's call, the one function it names as
highest value was decomposed and the issue closed.

`load_nif_bytes_with_skeleton`, **1115 → 298 LOC**, decomposed *in place*
along its own phase comments (a file split would have narrowed the three tests
that read `nif_loader.rs` as text):

| Function | LOC |
|---|---:|
| `load_nif_bytes_with_skeleton` | 298 |
| `spawn_nif_mesh` | 522 |
| `spawn_nif_particle_emitters` | 133 |
| `spawn_nif_nodes` | 109 |
| `attach_nif_skin_binding` | 108 |

**`spawn_nif_mesh` is still 522 LOC and remains a census entry.** It is the
per-mesh spawn — one coherent unit of work, now named — but it is not under
200, and saying otherwise would be the exact misreading #3858 was filed to
prevent. The five other functions in the census are untouched; two of them
(`build_and_upload_instances`, `record_skinned_blas_refit`) are render-
recording paths that per `feedback_speculative_vulkan_fixes.md` need RenderDoc
verification, not `cargo test`.
