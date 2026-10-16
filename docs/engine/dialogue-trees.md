# Dialogue trees: the greeting, line-lifetime, and force-greet layers

**Status**: Phases G, L and F **landed 2026-10-07** (#5367),
live-gated by
[`dt1-dialogue-layers.sh`](../smoke-tests/dt1-dialogue-layers.sh).
Remaining: Phase 4 (Skyrim force-greet behind the PKDT decode), the SM
`ADIA` tie-in, and Phase V (voice). This document was the scoping pass
for the dialogue remainder of M43's open scope and the `Dialogue` item
on M42's blocked-procedure list. The activation-driven core is already
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

- **Random** (#5397): positional, over *passing* candidates in
  priority/file order. The first passing INFO wins; if it is Random it
  opens a stack of the immediately following passing Random INFOs,
  ended by the next passing non-Random INFO (excluded) or a Random End
  INFO (`Flags 1` bit 5, included), and the stack rolls uniformly (CS
  wiki *Editing Dialogue*; GECK *Category:Dialogue*).
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
for every phase above, a blocker for none. **Landed 2026-10-08** —
see §5 phase 5.

## 5. Phases

1. **G — greetings. Landed 2026-10-07.** `open_conversation` falls
   back to the master's generic greeting topic (`GREETING` on FO3/FNV,
   `DialogueGenericHello` on Skyrim) when no owned quest topic
   qualifies — the quest-less-patron hole. Live gate
   (`dt1-dialogue-layers.sh` leg 1): activating Doc Mitchell on a
   fresh FNV boot selects the authored `GREETING` line (info 0x107222)
   where pre-#5367 selected nothing.
2. **L — line lifetime. Landed 2026-10-07.** `select_info` collects
   every passing candidate: the first one wins, and a Random first
   candidate rolls within its positional Random stack through a seeded
   `DialogueRandomState` (#5397 corrected the original rule, under which
   any passing non-Random INFO outranked an earlier Random set and Random
   End was ignored);
   spoken Say-Once INFOs are disqualified through the saved
   `DialogueSpokenInfoForms` set (save format v33; per speaker since
   #5300 / v38 — `(speaker reference FormID, INFO)` pairs, because the CK's
   rule is "once by any given Actor"; Skyrim's flag arrives via the INFO
   `ENAM` response-flags word, decoded for Skyrim only); a spoken Goodbye
   line ends its conversation when the presentation estimate elapses
   (the Late close check + the app-layer page close). All three pinned
   by unit tests.
3. **F — force-greet (FO3/FNV). Landed 2026-10-07.** `PKDD`'s topic
   FormID decoded onto `PackRecord::dialogue_topic` (census floor: 263
   of 332 FNV packages carry one); a `ForceGreetDirective` bridge walks
   the NPC to the player and opens the package's topic without
   activation, through the same selection path. Vanilla installs these
   packages from quest scripts the engine does not run, so the
   `dialogue.forcegreet` console door is the drivable installer
   (quest/alias installers adopt the same bridge when they carry
   one). Live gate (`dt1-dialogue-layers.sh` leg 2): PACK 0x000613BB
   (`CitGunnyGreetPlayer`) walks Doc Mitchell to the player and opens
   topic 0xC8 with no activation.
4. **Skyrim force-greet. Landed 2026-10-08; template instances
   2026-10-11 (#5511).** Skyrim authors force-greet as a `ForceGreet`
   leaf in the `PKCU` type-19 procedure tree, its topic riding the
   `Topic` data input's first type-0 `PDTO` FormID — the same authored
   role `PKDD` plays on FO3/FNV. A concrete package keeps its tree on
   the `PKCU` *template* and carries only data-input values, so
   `PackRecord::force_greet` takes the caller-resolved template (the
   `from_package` convention): the leaf is sought on the package's own
   tree first, then the template's, and the topic reads the instance's
   own `PDTO` at the leaf's `PKC2` indexes first, then the template's
   (the FO3/FNV procedure-15 byte can't false-positive on a tree pack:
   those read 18/19 and the classic enum stops at 16). Census
   (#5511, re-measured over Skyrim.esm's 5,961 PACKs): 339 force-greet
   packs — 5 own-tree templates plus 334 template instances, 320 of
   which author their own `PDTO` topic (e.g.
   `VigilantofStendarrForcegreetDaedric` 0x000CD613 → 0x000B30FA); 2
   templates author a topic
   (`dunWhiteRiverWatch_WatchmanForcegreetTemplate` 0x00108E85 →
   0x000812F2, `OrcGuardOutsideForcegreetPackage` 0x000BBAA4 →
   0x000BBA8B); the topic-less remainder (3 templates + 14 instances)
   generic-greets — the same authored remainder as FNV. 41 of the
   instances sit on NPC_ `PKID` default lists (87 edges — the
   vigilants, the cart drivers, Paarthurnax, the housecarls), but
   ambient selection still does not install them: the instance's
   condition lists are not modeled, the same #5376 fail-open gate the
   FO3/FNV arm applies. `dialogue.forcegreet` installs either dialect;
   `dt2-skyrim-forcegreet.sh` gates the resolution arms (own-tree and
   instance) and the activation-less open (DialogueGenericHello). The
   SM `ADIA` tie-in remains deferred with the ADIA-family decode
   (#5366 §3.1).
5. **V — voice. Landed 2026-10-08.** The authored FNV file-name
   convention is `sound\voice\<plugin>\<voice-type EDID>\<quest>_<topic>_<local
   id 8-hex>_<n>.ogg` — composed from the owning quest's and topic's
   EDIDs because INFO records routinely carry none of their own (the
   live-gated greeting 0x107222 has no EDID; its file is
   `vcg01_greeting_00107222_1.ogg`). #5395/#5521: the cut is a length test —
   only when quest + topic EDIDs exceed 25 characters is the quest cut to
   10 and the topic so the two total 25 (`vdoctors_doctormedical99ye`);
   at or under 25 both keep their full EDIDs
   (`dialoguerivetcity_greeting`), radio quests included — one candidate,
   no fallback probe; the id is plugin-local (top byte `00`, DLC archives
   included); `<n>` is each segment's authored response number. The
   voice type resolves NPC → `VTCK` → indexed `VTYP` EDID; the plugin
   segment is the INFO's owning plugin through `GlobalFormIdResolver`
   (#5393 — masters own the low slots). FNV DLC voice ships in
   `<DLC> - Main.bsa`, which no profile mounts; pass it with
   `--sounds-bsa`. `Fallout - Voices1.bsa`
   joined the FNV profile's sounds list (105,517 entries — its first
   consumer), and `apply_selection` resolves/decodes/schedules the
   segments sequentially (kira delayed start) at the NPC, with the
   total voice duration replacing the Phase-L subtitle estimate in the
   Goodbye close. A line with no resolvable voice keeps the estimate —
   missing audio is the common modded shape, not an error. `.fuz`
   (Skyrim+) is a different container (`FUZE`, u32 version, u32 lip size,
   raw `.lip` bytes, then a RIFF **xWMA** audio stream — not XMA2;
   `skyrim_fuz_container_shape`, #5464) — deliberately V2 scope (tracked by #5912); its
   decoder is xWMA. Live gate (`dt1-dialogue-layers.sh` leg 3):
   the fresh-boot greeting resolves 1 segment / 5.0 s in
   `maleuniquedocmitchell`.
   **Scope: this is the FO3/FNV convention, not a universal one.** Oblivion
   is out of scope (#5526): `Oblivion.esm` authors 0 `VTCK` on its 2,482 NPC_
   and 0 `VTYP` records, so the voice-type lookup is always `None`, and its
   voices (`Oblivion - Voices1/2.bsa`: 38,898 `.mp3` + 38,882 `.lip`) are
   `sound\voice\oblivion.esm\<voice race full name, lowercased>\<m|f>\<quest
   EDID>_<topic EDID>_<formid 8-hex>_<n>.mp3` — **not** truncated (quests run
   to 22 characters, quest + topic to 46). The race folder is the speaker's
   race mapped through RACE `VNAM` (by sex) to the voice race's FULL name
   (`RaceRecord::voice_forms`, decoded, no consumer yet). An Oblivion line
   keeps the Phase-L subtitle estimate until a per-game voice-path table
   entry lands.

## 6. What this document does NOT decide

- The per-game greeting identification rule (Phase G's alignment pass).
- Whether SayOnce state lives in the quest-state save family or a
  sibling resource — an implementation detail at Phase L.
- Combat/detection/service bark scheduling (bark systems are their own
  M42/M47 surface, not the activation conversation).
- Skyrim's INFO `DATA` minority layout (924 records) beyond the flags
  the runtime consumes.
