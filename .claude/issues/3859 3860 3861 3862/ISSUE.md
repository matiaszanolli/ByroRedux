# Bundle: #3859 #3860 #3861 #3862

Four tech-debt findings from `AUDIT_TECH_DEBT_2026-09-05` at `fa5c4191`. All
premises verified; two had drifted in ways worth recording.

| # | Finding | Outcome |
|---|---|---|
| 3859 | 105-arm FourCC→FormType match | table + invariant tests |
| 3860 | 12 hand-rolled image chains | `GpuImage` + 14 migrations, 15 commits |
| 3861 | `ImageSpaceModifier{Frame,View}` twins | hoisted to `crates/core` |
| 3862 | `FloatTarget`/`ColorTarget` duplicated | re-exported from core |

## Premise drift found

- **#3859** — `crates/sdk/src/compatibility.rs` is now
  `compatibility/storage_util.rs`, and the match had grown 105 → **106** arms.
- **#3860** — the site list was 12; it is **14**. `context/helpers.rs` and
  `groundcover_bench.rs` appeared since the audit, and `volumetrics.rs` became
  `volumetrics/init.rs`.

## What each fix turned out to be

**#3859** is the smallest and the clearest win-per-line: 106 match arms cannot
be asked whether they are sorted, whether a signature repeats, or how many
there are. A `static` table can, and `binary_search_by_key` silently returns
wrong answers on an unsorted slice — so the sortedness test is load-bearing,
not decorative. The mapping was verified byte-identical before and after.

**#3861** and **#3862** are the same shape: a type duplicated across a crate
boundary with a hand-written identity bridge. Both bridges are deleted. For
#3862 the gate is *compiler-enforced* type identity (assignment across the two
paths only compiles while they name one type), which is better than a source
scan because redeclaring the enum breaks the build at the moment it happens.

**#3860** is the substantial one — see below.

## #3860 in detail

One ~300-line `GpuImage` replaces fourteen copies of an ~85-line chain. The
finding was never really about the line count: it is that the cleanup ordering
and the allocator-lock scope had to be re-derived at each copy, so **one defect
got fixed four times** (#1163, #1164, #1165, #2178) and #2178's comment records
its author hand-checking three sibling copies to land one fix.

Sequenced as 15 commits — the type, then one per file — with a `PENDING` ledger
in the gate that each migration commit shrinks by one line, and a second
assertion that fails on a stale entry so the list cannot rot into a blanket
exemption.

**Three things the migration found that the issue did not predict:**

1. **`water_caustic.rs` carried #2779's defect, unfixed.** Its `storage_view`
   and `sampled_view` were produced by the *same closure* — same image, view
   type, format, subresource range — and the code said so. #2779 established
   for `caustic.rs` that a view carries no usage or layout state; the same
   reasoning holds verbatim here. Migrating collapses them, so each frame in
   flight stops paying for a redundant `VkImageView` and a second destroy.
2. **Two leak safety nets could only scream.** `svgf.rs`'s `HistorySlot::Drop`
   and `gbuffer.rs`'s `Attachment::Drop` both documented that they stashed no
   device or allocator and therefore "can't clean up; it can only scream"
   (REN-D2-NEW-01). `GpuImage` holds cheap Arc-backed clones of both, so those
   escapes are now *recovered*. `gbuffer`'s was the renderer's largest leak: up
   to 42 handles per GBuffer.
3. **Parallel Vecs were the real hazard, not the chain.** `ssao`, `gbuffer`,
   `composite` and `frame_upscaler` each kept images, views and allocations in
   separate Vecs whose indices had to stay in step by hand — and #1164 and
   #2178 are exactly what happens when they come apart. One `Vec<GpuImage>`
   makes the correspondence structural.

**Two deliberate limits:**

- `texture.rs` is not migrated and is on the gate's ALLOWED list with its
  reason: it uploads host data, generates mip chains and drives its own layout
  transitions, none of which `GpuImage` models.
- `context/helpers.rs` uses a new `GpuImage::into_parts` to keep returning the
  raw triple. Its results are three flat `VulkanContext` fields freed by
  `destroy_depth_resources`; reworking those is a `VulkanContext` field-count
  question (#3736) across ~108 references. The chain is consolidated; the
  storage is left where it is.

**Source-shape tests moved with the code they guard.** #2178's pin lived in
`frame_upscaler.rs` and asserted the free-before-destroy ordering once per copy
(one scan of that file, one of `gbuffer.rs`). Both targets are gone, so it is
now a single assertion over `GpuImage::create`'s bind *and* view arms — broader
than the two it replaces, and unable to go stale one copy at a time.
