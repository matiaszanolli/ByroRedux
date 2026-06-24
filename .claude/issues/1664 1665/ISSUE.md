# #1664 — GetDistance (idx 36) + #1665 — GetFactionRank (idx 60)
Domain: ecs/scripting (byroredux-scripting + byroredux-core, + binary npc_spawn for #1665).
Last two stubs from #1316 cluster.

## #1664 GetDistance
Returns 0.0. param_1 is a target FormID. NOTE: #1666 already remaps param_1
to global space at parse for idx 36 (param1_is_form_id catalog). Remaining:
resolve global param_1 → EntityId, then
dist = ‖GlobalTransform(subject) − GlobalTransform(target)‖.

## #1665 GetFactionRank
Returns -1.0 (not-in-faction sentinel). Needs FactionMembership component
(faction FormID → rank), populated at NPC spawn from NPC_ faction data.
Then return Run-On's rank in param_1's faction, or -1. param_1 already
remapped global (idx 60 in catalog). Mirrors PerkList work for HasPerk.
