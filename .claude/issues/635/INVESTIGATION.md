# Investigation — #635 (FNV-D3-LOW bundle)

## Domain
binary (cell loader)

## D3-05 verification — NifImportRegistry LRU gap
- `byroredux/src/cell_loader.rs:80–94` — `NifImportRegistry` HashMap-only,
  no eviction policy.
- `cell_loader.rs:104–108` — `clear()` already exists for hard reset, not
  wired into `unload_cell`.
- `cell_loader.rs:192–357` — `unload_cell` releases GPU resources but never
  touches the registry. Confirmed.
- `commands.rs:243–271` — `mesh.cache` debug command already wired (audit
  recommendation b's visibility piece is complete).
- Cap policy: not authored. The audit gives no specific value; FNV ships
  ~14 k unique NIFs and Skyrim SE far more, with per-cell working sets in
  the low thousands. **No-guessing rule applies** — defaulting to
  unlimited preserves today's behavior; an opt-in `BYRO_NIF_CACHE_MAX`
  env var lets operators clamp without picking an arbitrary number for
  everyone.

## D3-06 verification — single-level PKIN expansion
- `byroredux/src/cell_loader.rs:1925–1946` — `expand_pkin_placements` walks
  `pkin.contents` once and returns leaf form IDs. No recursion.
- `cell_loader.rs:1398–1432` — caller drops any synth child whose form ID
  isn't in `index.statics`. PKIN-of-PKIN, SCOL-of-PKIN, LVLI-of-PKIN all
  silently miss.
- Vanilla FO4 has no PKIN-of-PKIN (`audit FO4-DIM4-03` baseline). FNV
  ships zero PKIN content. Confirmed forward-looking gap.

## Sibling check — LVLI / SCOL composition
- `expand_scol_placements` (line 1972) is also single-level by design; the
  audit explicitly notes vanilla FO4 has no SCOL-of-SCOL, and the doc
  comment at line 1969–1971 already pins the limitation.
- `LVLI` expansion is **not implemented at all** today (`stat_miss` log
  comment at line 1620 acknowledges leveled-list targets fall through).
  Tracked via #386 — out of scope for this issue.

## Scope decision
Two single-file fixes, ~120 lines including tests:

1. **D3-05**: opt-in LRU cap in `NifImportRegistry`. Env var
   `BYRO_NIF_CACHE_MAX` (default 0 = unlimited preserves today's
   behavior). LRU tracked via per-key access tick; eviction on insert
   when over cap. Eviction count surfaced via `mesh.cache`.

2. **D3-06**: depth-bounded recursive PKIN expansion. Depth cap = 4
   (per audit). Children that resolve to another PKIN fan out further;
   non-PKIN children fall through unchanged. SCOL-of-PKIN and
   LVLI-of-PKIN remain dropped (separate, larger fixes — #386 / #585
   territory) but explicitly documented.

Touches: `byroredux/src/cell_loader.rs` +
`byroredux/src/cell_loader_nif_import_registry_tests.rs`. Two files,
within the 5-file scope cap.
