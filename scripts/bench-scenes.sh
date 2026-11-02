#!/usr/bin/env bash
# Benchmark scene definitions shared by scripts/fsr-bench-matrix.sh and the
# M-LIGHT baselines (scripts/m-light/baselines.py): each scene's working
# directory, its load arguments and its near-empty-load entity floor.
# Source it; it defines functions only. Needs REPO and GAMES_ROOT.

: "${REPO:?bench-scenes.sh needs REPO}"
: "${GAMES_ROOT:?bench-scenes.sh needs GAMES_ROOT}"

# Scene definitions: name, working directory, and load arguments. The three
# game scenes are the existing bench-of-record trio so the numbers stay
# comparable with ROADMAP's history; Cornell is the redistributable control
# that anyone can reproduce without game data.
# Minimum plausible entity count per scene (#3347).
#
# The documented failure mode in the header — archives silently failing to open
# and the scene loading near-empty — still emits a `bench:` line, so the run
# loop's "did a bench line appear" check accepted it as data. A near-empty
# Prospector loads ~36 entities and reports a spurious ~1792 FPS.
#
# Floors are ~50% of the counts this harness actually recorded in the
# bench-of-record table (ROADMAP: Prospector 3757, Whiterun 5183, MedTek 32920,
# Dugout 7346, Cornell 37) — loose enough that legitimate content-scope drift
# does not trip them, tight enough that a near-empty load cannot pass.
#
# NOTE the floors must be per-scene, not global: Cornell is a 37-entity
# synthetic control, i.e. the same order as a *failed* game-scene load. A single
# global floor would either miss the failure or reject Cornell outright.
scene_entity_floor() {
  case "$1" in
    cornell)    echo 20 ;;
    prospector) echo 1800 ;;
    whiterun)   echo 2500 ;;
    medtek)     echo 16000 ;;
    dugout)     echo 3600 ;;
    # #3467 — UNCALIBRATED. Every other floor here was set from a measured
    # run; this one has never been run, so 0 is a placeholder, not a
    # threshold, and `gridcross` is deliberately NOT in the default SCENES
    # list below. Calibrate it on the first successful run (take the observed
    # entity count, round down generously) and add it to the default set then
    # — shipping an uncalibrated floor in the default matrix would let a
    # near-empty exterior load pass as a valid bench row, which is exactly
    # what the per-scene floors exist to catch.
    gridcross) echo 0 ;;
    *)          echo 0 ;;
  esac
}

scene_dir() {
  case "$1" in
    cornell)    echo "$REPO" ;;
    prospector) echo "$GAMES_ROOT/Fallout New Vegas/Data" ;;
    whiterun)   echo "$GAMES_ROOT/Skyrim Special Edition/Data" ;;
    medtek)     echo "$GAMES_ROOT/Fallout 4/Data" ;;
    dugout)     echo "$GAMES_ROOT/Fallout 4/Data" ;;
    gridcross)  echo "$GAMES_ROOT/Fallout New Vegas/Data" ;;
  esac
}

# Argument arrays are built per scene rather than by word-splitting a string,
# because every vanilla archive name contains spaces.
run_scene_args() {
  local scene="$1"
  case "$scene" in
    cornell)
      ARGS=(--cornell)
      ;;
    prospector)
      # #5257 — `Update.bsa` mounts LAST in both pools, mirroring the
      # `--game fnv` profile's lists (last-listed wins, #3637/#3896). None
      # of its 55 NIFs are Prospector content, so the bench-of-record
      # numbers are unchanged; land this change byte-identically across
      # the next same-machine control so the record stays comparable.
      ARGS=(--esm FalloutNV.esm --cell GSProspectorSaloonInterior
            --bsa "Fallout - Meshes.bsa"
            --bsa "Update.bsa"
            --textures-bsa "Fallout - Textures.bsa"
            --textures-bsa "Fallout - Textures2.bsa"
            --textures-bsa "Update.bsa")
      ;;
    whiterun)
      ARGS=(--esm Skyrim.esm --cell WhiterunBanneredMare
            --bsa "Skyrim - Meshes0.bsa" --bsa "Skyrim - Meshes1.bsa")
      local i
      for i in 0 1 2 3 4 5 6 7 8; do
        ARGS+=(--textures-bsa "Skyrim - Textures$i.bsa")
      done
      ;;
    medtek)
      ARGS=(--esm Fallout4.esm --cell MedTekResearch01
            --bsa "Fallout4 - Meshes.ba2" --bsa "Fallout4 - MeshesExtra.ba2")
      local i
      for i in 1 2 3 4 5 6 7 8 9; do
        ARGS+=(--textures-bsa "Fallout4 - Textures$i.ba2")
      done
      ARGS+=(--textures-bsa "Fallout4 - TexturesPatch.ba2"
             --materials-ba2 "Fallout4 - Materials.ba2")
      ;;
    dugout)
      ARGS=(--esm Fallout4.esm --cell DmndDugoutInn01
            --bsa "Fallout4 - Meshes.ba2" --bsa "Fallout4 - MeshesExtra.ba2")
      local i
      for i in 1 2 3 4 5 6 7 8 9; do
        ARGS+=(--textures-bsa "Fallout4 - Textures$i.ba2")
      done
      ARGS+=(--textures-bsa "Fallout4 - TexturesPatch.ba2"
             --materials-ba2 "Fallout4 - Materials.ba2")
      ;;
    # #3467 — the first EXTERIOR scene in the matrix. Every other entry is an
    # interior `--cell`, which is why the resumable global-geometry rebuild
    # (`GEOMETRY_REBUILD_CHUNK_BYTES`) has no bench coverage at all: it only
    # engages once streaming grows the geometry pool past a few hundred MB,
    # which interiors never do. Without a row here, re-picking that constant
    # against the new `geom_rebuild` timing cannot be regression-gated.
    #
    # FNV + `--grid 0,0 --radius 3` is the documented exterior invocation
    # (README / CLAUDE.md); radius 3 is a 7x7 grid, enough traversal to grow
    # the pool without making a bench run take minutes.
    gridcross)
      ARGS=(--esm FalloutNV.esm --grid 0,0 --radius 3
            --bsa "Fallout - Meshes.bsa"
            --textures-bsa "Fallout - Textures.bsa"
            --textures-bsa "Fallout - Textures2.bsa")
      ;;
    *)
      echo "unknown scene '$scene'" >&2
      return 1
      ;;
  esac
}
