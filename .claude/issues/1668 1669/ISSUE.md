# #1668 — resolve Global comparand (CTDA use_global)
Domain: ecs/scripting (byroredux-scripting + byroredux-plugin + binary).
`evaluate_condition` returns 0.0 for a `ConditionValue::Global(form_id)`
comparand. Blocker: `EsmIndex` (holds `globals: HashMap<u32, GlobalRecord>`)
doesn't impl Resource and is never inserted into World. Need: a Globals
lookup as a World Resource + remap-consistent key; then
`comparand = globals[form_id].value`.

# #1669 — TD9-NEW-02: split asset_provider.rs (3014 LOC)
Domain: binary (byroredux). File past 2000-LOC ceiling. Split axis:
BSA/BA2 archive resolution · TextureProvider impl · mesh extraction.
Behavior-preserving; no circular imports; tests pass.
