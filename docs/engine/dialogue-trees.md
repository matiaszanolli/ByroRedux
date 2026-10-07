# Dialogue trees: the greeting, line-lifetime, and force-greet layers

**Status**: PROPOSED (2026-10-07). No code lands from this document by
itself — it is the scoping pass for the dialogue remainder of M43's
open scope ("the dialogue tree with its UI") and the `Dialogue` item on
M42's blocked-procedure list. The activation-driven core is already
shipped; this doc scopes what the corpus proves is missing on top of
it. The Story Manager landed first (#5366 Phases 0–1) because SM events
are dialogue-adjacent producers; this is its natural follower.

## 1. What already exists (do not re-scope)

The P4 activation route plus the 2026-10 dialogue cluster is more
complete than the roadmap's one-liner credit:

- **Branch model, blocking included** (#5037): `topic_entry` in
  `byroredux/src/systems/npc_dialogue.rs` resolves each owned topic as
  `TopLevel` / `Blocking` / `LinkOnly` from `DLBR` (+ FO3/FNV DIAL
  `DATA` top-level flag, #5224); a qualifying Blocking entry pre-empts
  the opening selection and its `TCLT` links become the list. **The
  blocking branch is modeled** — the old roadmap phrase "the force-greet
  blocking branch is unmodeled" conflated two things: the blocking
  branch (modeled) and force-greet (the package-driven opener — not
  modeled, §5 Phase F).
- **Menus + selection**: top-level lists, linked lists, one-live-
  selection (#5038), condition-evaluated INFO choice with per-INFO
  quest ownership and priority ordering (#5271,
  `select_info` in `crates/scripting/src/dialogue.rs`), UI re-selection
  door, refusals, the native dialogue page (debug-UI) with its
  open-once cue, and OnBegin/OnEnd `TIF_` fragment dispatch (#5152).
- **Parse side is deep**: typed `InfoDataHeader` (Goodbye / Random /
  SayOnce bits, #4469/#5295), response segments with emotion +
  per-segment text (#3616), `ConversationTree` PNAM chains + TCLT
  topic links, `DialogueCategory` (Topic/Conversation/Combat/
  Persuasion/Detection/Service/Misc/Radio/Scene), voice-file naming
  (see §6 of `InfoRecord`'s docs), DLBR branches.

## 2. What the corpus says is missing (2026-10-07 census)

| Master | DIAL | INFO | greeting DIAL / INFO¹ | Dialogue `PACK`s² | greeting INFOs w/o conditions |
|---|---:|---:|---:|---:|---:|
| `Skyrim.esm` | 15 037 | 31 465 | 345 / 2 578 | ≥260 (EDID-named³) | 10 |
| `FalloutNV.esm` | 18 215 | 23 247 | 447 / 6 643 | 332 | 134 |
| `Fallout3.esm` | 6 381 | 22 327 | 374 / 5 612 | 386 | 118 |

¹ DIALs whose EDID/FULL matches greet/hello — the identification rule
is an alignment item (§5 Phase G), not settled by this heuristic.
² FO3/FNV `PKDT` procedure byte 15 (Dialogue) — the field the engine
already decodes (`PackRecord::procedure_type`).
³ Skyrim's `PKDT` byte 4 is **not** the procedure field (only values
18/19 appear across all 5 961 packs — a flags byte; measured, not
guessed); Skyrim force-greet scale is bounded below by the 260 packs
whose EDID names one, and the true count waits on the Skyrim PKDT
procedure decode that `misc/pack.rs` defers to the AI runtime.

Line-lifetime flags, FNV `DATA.Flags1` distribution (same shape on
FO3): Goodbye bit on ~7 700 INFOs, Random on ~5 500, SayOnce on
~3 500 — and `select_info` reads none of them: it picks the
first-passing INFO, replays it forever, and never ends a conversation
from a spoken line (conversation end today is only the dialogue page's
Close/Escape, `main.rs`'s `end_open_conversation` call).

**The player-visible hole**: `npc_dialogue.rs`'s own scope note — "a
patron bound to no running quest selects nothing." Activation on a
quest-less NPC finds no owned topics and produces no conversation,
because the greeting layer that vanilla leads every conversation with
is the missing subsystem.

## 3. Scope

In scope: the greeting pass, line-lifetime semantics (Random / SayOnce
/ Goodbye), force-greet (FO3/FNV procedure-15 packages first), and the
audio follow-on. Out of scope: persuasion/speech-challenge minigame,
services (barter/repair), the SCEN bark categories (already consumed by
scene playback), and any menu restyle — the native dialogue page is the
UI for all of this.

## 4. Design

### G — greeting layer

On activation, before the running-quest topic walk, scan the NPC's
owned **greeting set** — the Topic-category DIALs matching the
per-game greeting identification rule — and `select_info` over it with
the same evaluator discipline. Passing greeting becomes the opening
line; the following list is the top-level menu exactly as today. The
identification rule is Phase G's alignment item: candidate rules
(EDID/FULL convention per game, FO3/FNV's literal `GREETING` EDID
family, Skyrim's greeting topic set) get the same corpus verification
the SM mnemonics got — measured against every greeting DIAL the census
found, with the misses named.

### L — line lifetime

- **Random**: among *passing* candidates whose Random bit is set,
  pick uniformly instead of first (vanilla semantics; non-random
  candidates keep priority order ahead of them).
- **SayOnce**: a per-save spoken-set keyed by INFO FormID joins the
  save alongside quest state; a spoken SayOnce INFO stops qualifying.
- **Goodbye**: when the *chosen* line carries the Goodbye bit, the
  conversation ends when its presentation finishes (OnEnd fragments
  run, selection clears, the page closes) instead of waiting for the
  player to close it.

### F — force-greet

FO3/FNV procedure-15 packages: resolve the package's dialogue target
(`PTDT`/topic decode — the piece `misc/pack.rs` defers), walk to the
player with the existing locomotion, then `open_conversation` with the
package's topic pre-selected, bypassing activation. Skyrim follows its
PKDT procedure decode (§2 footnote); its SM tie-in is the unmapped
`ADIA`-family event mnemonics (#5366 §3.1), the same dispatch this
would consume.

### V — voice

`InfoRecord` already carries voice-file naming; the audio runtime
(kira) has no dialogue consumer, and FNV's Voices BSA is deliberately
unopened (`profiles.toml`'s "no dialogue consumer exists yet"). Wire
per-line voice playback when a line is presented. Last: it is polish
for every phase above, a blocker for none.

## 5. Phases

1. **G — greetings** (FO3/FNV first: 5–6.6k lines each, 118–134
   unconditional so a fixture needs no condition machinery). Gate: a
   quest-less FNV Prospector patron greets on activation with a
   passing greeting line — today it selects nothing.
2. **L — line lifetime**. Gate: a Random greeting varies across
   activations in a pinned-seed test; a Goodbye line ends its
   conversation without the page's Close; a SayOnce line does not
   repeat across an F5/F9 cycle.
3. **F — force-greet (FO3/FNV)**. Gate: a live route where a
   procedure-15 package walks an NPC to the player and opens its
   topic without activation.
4. **Skyrim force-greet + SM `ADIA` tie-in** (behind the Skyrim PKDT
   procedure decode). 5. **V — voice.**

## 6. What this document does NOT decide

- The per-game greeting identification rule (Phase G's alignment pass).
- Whether SayOnce state lives in the quest-state save family or a
  sibling resource — an implementation detail at Phase L.
- Combat/detection/service bark scheduling (bark systems are their own
  M42/M47 surface, not the activation conversation).
- Skyrim's INFO `DATA` minority layout (924 records) beyond the flags
  the runtime consumes.
