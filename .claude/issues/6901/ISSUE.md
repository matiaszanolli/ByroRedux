MUS verify: the Success Music event, and whether music resumes or restarts after a sting

Part of #6860, filed by Phase 0 (#6867). PRD [MUS-STG-003, MUS-STG-004](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#54-stings-mus-stg).

Fallout 3 and New Vegas keep a Success Music default object (`DefaultSuccess`, `special\\success.mp3`) beside Death and Level Up; GECK *Default Form* lists the slot but not the event that plays it.

**Verify, against the original:** which event plays Success Music (quest completion is the likely candidate); whether Battle and Location music resume where they were after the death or level-up sting, or restart.

**Engine default until then:** Success Music is bound to no event; a sting pauses the Battle and Location layers and they resume where they stopped (`docs/engine/music.md` §5).
