# #6896 / #6897 / #6898 — Bink verify items, read out of the executables (2026-11-17)

Issues #6897 and #6898 share this note. Executables: Oblivion.exe and Fallout3.exe (plain `.text`, base
0x400000), Fallout4.exe (plain `.text`, 64-bit, base 0x140000000). Method: [[fallout3-exe-disassembly]]
(capstone; IAT slot -> `call [slot]`/`ff 15` scan; setting value slot = name-pointer slot - 4).

## #6896 — audio track
- Oblivion/FO3 import no `BinkSetSoundTrack`; their `BinkOpen` flags are `0x2000` (preload) | `0x8000000`
  (no-MMX), never `BINKSNDTRACK` (`0x4000`). Every movie of theirs has <= 1 track.
- Skyrim SE imports `BinkOpenDirectSound`, `BinkSetSoundSystem` only. `BGS_Logo.bik` tracks (IDs 1, 0)
  decode identically (ffmpeg, 668160 samples, 0 differ).
- Fallout 4: wrapper `0x1debb50(id, &flags)` = `*flags |= 0x4000; BinkSetSoundTrack(1, &id)`, called from
  the movie open `0x1ded080` with `id = request[0]`. The request builder is `0x1deb730`; its stack argument
  is `0x22b5c40(language_index)`: `>5 ? (==8 ? 8 : 1) : idx`. The index is `0x22b5bd0(sLanguage)`, a
  `_stricmp` walk of the table at `0x291eda0`: mul 0, en 1, fr 2, it 3, de 4, es 5, esmx 6, pl 7, ja 8, ...
  Hence en 1, fr 2, it 3, de 4, es 5, ja 8, everything else 1. The no-argument start path `0x1dec870` is
  hard-wired to 1. FO4's 5-track films have IDs 1..5 in file order; `GameIntro_V3_B` has IDs 0..5 and six
  identical tracks (binkaudio `examples/tracks`), so first-track vs ID 1 never differed in the installed
  corpus; the code now selects by ID anyway.
- Do NOT parse KB2j (Fallout 76 / Starfield) track IDs with the Bink 1 header layout: it yields a bogus
  first track (rate 14848, ID 0x70000000). Use `crates/bink2` `Bk2::tracks`.

## #6897 — when `sIntroMovie` / `sOblivionIntro` play
- `OblivionIntro.bik` is the default of `sOblivionIntro:General` (setting entry `0xb030a0`; value slot
  `0xb030a4`; the name string follows the file name at `0xa31b00`/`0xa31b14`). The doc's "bare string, not a
  setting" was wrong. `Oblivion_default.ini` does not set it.
- Oblivion: `0x5b5df0` (logs "Unable to find character generation quest '%s'.") sets `player+0x7f8 = 1`;
  `0x66b041` (player init) clears it; the cell-change routine `0x66eaf0` plays the movie at `0x66f2be`
  (`0x410ba0(sOblivionIntro, ...)`) under `[player+0x7f8]` and then clears it.
- FO3: setting entry `0x111566c`, value slot `0x1115668` (compiled-in `""`); `0x9671b0` (menu callback,
  reads `sCharGenQuest`) sets global `0x1239beb = 1`; `0x93ea48` (return to main menu) clears it;
  `0x8bfac0` plays `sIntroMovie` at `0x8c071f` under it. What limits FO3 to one play is not decoded.
- Movie play `0xb5da00` (FO3) arguments, matched to the script command (`0x79ead0`, locals defaulted 0/1/1/1
  in arg order): P2 interruptable, P3 preload, P6 letterbox, P8 mute audio, P9 pause music. `sIntroMovie`:
  P2, P3, P6, P8 and P9 are all 1. `sMainMenuMovieIntro` (`0x9b3c66`, setting `0x111565c`): interruptable, muted,
  music-paused, NOT letterboxed (P6 = 0).

## #6898
- Letterbox clear: FO3 draw `0xb5efb0`: `if (!letterbox && [0x131a446]) set viewport = whole render target`
  (rect from `0x11ff658..64` = 0,1,1,0), restored at `0xb5f26c`. The movie is a tiled textured quad
  (`0xb5fb40`, scale `1,1`, offset `0,0` after reset; writers of those fields not traced).
- Archives: Oblivion `0x4101f9`, FO3 `0xb5ee0f` (`0xd87317`), FO4 `0x1ded14d` (`_stat64i32`) check the file on
  disk, then `BinkOpen(path, flags)` without `BINKFROMMEMORY`. FO4 tries the request's explicit path, then
  `Data\Video\<name>`. Skyrim SE / Starfield not read (Skyrim SE `.text` not decoded here).

## Not done
- No new-game hook for FO3/Oblivion (`--new-game` exists only for FNV, [[fnv-new-game-route]]); the doc
  records the arguments to queue.
