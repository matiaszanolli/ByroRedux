# #1666 + #1667 — M47.1 condition stubs: GetIsID + HasPerk

Both decomposed from #1316 (TD5-NEW-01). Domain: **ecs** (`byroredux-core`),
with parser work in `byroredux-plugin` and the evaluator in `byroredux-scripting`.

## #1666 — GetIsID (function index 71)
Returns `0.0` today. Real blocker: `param_1` is parsed raw / plugin-local
(`parse_ctda`), but an entity's `FormIdComponent` resolves (via `FormIdPool`)
to a global load-order `FormIdPair`. A "lower-24-bits" shortcut is a
multi-plugin false-positive landmine. Needs a CTDA form-id remap resolver.

## #1667 — HasPerk (function index 99)
Returns `0.0` today. Needs a per-actor `PerkList` ECS component; then return
`1.0` iff it contains `param_1`.

## Approach (user-approved: full fix)
1. `byroredux-plugin` condition.rs: `remap_condition_form_ids()` + catalog gate
   `param1_is_form_id()` — promote CTDA form-id fields (param_1 for the form-id
   functions, reference_form_id, Global comparand) from plugin-local → global,
   using the owning plugin's `FormIdRemap`. Applied at the 3 live parse_ctda
   sites (parse_qust, parse_info, parse_perk).
2. `byroredux-core`: new `PerkList` component.
3. `byroredux-scripting` condition.rs: implement GetIsID (entity FormId pair
   local == param_1) and HasPerk (PerkList contains param_1), both via FormIdPool.
