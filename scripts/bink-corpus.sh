#!/usr/bin/env bash
# Bink movie corpus census (PRD bink-video-playback §3, BIK Phase 0, #6856).
#
# For every installed title, counts the movies under its `Data\Video` folder,
# splits them into Bink 1 (`BIK` magic) and Bink 2 (`KB2` magic), and reports
# the range of audio tracks per file. Reads game files in place; copies none.
#
#   scripts/bink-corpus.sh                     # every title under the default Steam library
#   BYROREDUX_STEAM_COMMON=/path scripts/bink-corpus.sh
#
# Needs `ffprobe` (FFmpeg) for the track counts; without it the column is "?".
set -euo pipefail

BASE="${BYROREDUX_STEAM_COMMON:-/mnt/data/SteamLibrary/steamapps/common}"
have_ffprobe=0
command -v ffprobe >/dev/null 2>&1 && have_ffprobe=1

# title|install dir|video dir (case as shipped)
GAMES=(
    "Oblivion|Oblivion|Data/Video"
    "Fallout 3|Fallout 3 goty|Data/Video"
    "Fallout: New Vegas|Fallout New Vegas|Data/Video"
    "Skyrim SE|Skyrim Special Edition|Data/Video"
    "Fallout 4|Fallout 4|Data/Video"
    "Fallout 76|Fallout76|Data/Video"
    "Starfield|Starfield|Data/video"
)

echo "| Game | Files | Bink 1 | Bink 2 | Audio tracks per file |"
echo "|---|---|---|---|---|"
for entry in "${GAMES[@]}"; do
    IFS='|' read -r title dir video <<<"$entry"
    path="$BASE/$dir/$video"
    if [[ ! -d "$path" ]]; then
        echo "| $title | not installed | | | |"
        continue
    fi
    files=0 bink1=0 bink2=0 min=-1 max=-1
    for file in "$path"/*; do
        [[ -f "$file" ]] || continue
        files=$((files + 1))
        magic=$(head -c 3 "$file")
        case "$magic" in
            BIK) bink1=$((bink1 + 1)) ;;
            KB2) bink2=$((bink2 + 1)) ;;
        esac
        if ((have_ffprobe)); then
            tracks=$(ffprobe -v error -select_streams a -show_entries stream=index -of csv=p=0 "$file" 2>/dev/null | wc -l)
            ((min < 0 || tracks < min)) && min=$tracks
            ((tracks > max)) && max=$tracks
        fi
    done
    if ((have_ffprobe)); then
        range=$([[ $min -eq $max ]] && echo "$min" || echo "$min–$max")
    else
        range="?"
    fi
    echo "| $title | $files | $bink1 | $bink2 | $range |"
done
