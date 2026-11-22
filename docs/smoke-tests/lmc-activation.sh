#!/usr/bin/env bash
# Headless live gate for plugin-profile activation (PRD legacy-mod-compatibility Phase 1, #6670): `lmc-activation.sh`.
# No window and no Vulkan: each leg runs `--list-cells` (the last `--plugin-report`) over New Vegas masters plus small generated plugins and reads
# the activation report the engine logs.
#   1. masters out of order: DeadMoney.esm listed before FalloutNV.esm is hoisted behind it (LMC-ID-003) and loads; the
#      generated OtherCase.esp brings the `OtherCase - Assets.bsa` beside it, not `OtherCase_Assets.bsa`, and the
#      DeadMoney.esm master its two archives (LMC-AST-002).
#   2. a missing master: a plugin naming LonesomeRoad_absent.esm is disabled with the patch that depends on it, a plugin
#      naming its master in another case (FALLOUTNV.ESM) is kept (LMC-ID-001, -009, -011), and the session loads.
#   3. the session plugin itself missing a master: DeadMoney.esm alone is refused, naming the --master to pass.
#   4. the conflict report: `--plugin-report 000000D2 fields` names the Dead Money topic's carriers in load order and the
#      sub-records Dead Money changed against FalloutNV.esm (LMC-REC-007), after the archives Dead Money pairs with.
# Requires the New Vegas data with the Dead Money and Caravan Pack DLC (BYROREDUX_FNV_DATA), python3 and a built engine
# (BYRO_BIN, else target/release, else target/debug).
set -euo pipefail

GATE=lmc-activation
DATA="${BYROREDUX_FNV_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data}"
if [[ -z "$DATA" || ! -f "$DATA/FalloutNV.esm" || ! -f "$DATA/DeadMoney.esm" || ! -f "$DATA/CaravanPack - Main.bsa" ]]; then
    echo "smoke[$GATE]: SKIP -- missing BYROREDUX_FNV_DATA with Dead Money and Caravan Pack (set it to the New Vegas Data directory)"
    exit 77
fi
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="${BYRO_BIN:-}"
if [[ -z "$BIN" ]]; then
    for candidate in "$ROOT/target/release/byroredux" "$ROOT/target/debug/byroredux"; do
        [[ -x "$candidate" ]] && { BIN="$candidate"; break; }
    done
fi
if [[ -z "$BIN" || ! -x "$BIN" ]]; then
    echo "smoke[$GATE]: SKIP -- missing engine binary (cargo build -p byroredux, or set BYRO_BIN)"
    exit 77
fi
command -v python3 >/dev/null || { echo "smoke[$GATE]: SKIP -- missing python3"; exit 77; }

WORK="$(mktemp -d "/tmp/byro-$GATE.XXXXXX")"
echo "smoke[$GATE]: engine $BIN, logs in $WORK"
# A plugin is a TES4 record with a New Vegas HEDR and its MAST list; activation reads nothing else.
python3 -I - "$WORK" <<'EOF'
import os, struct, sys
out = sys.argv[1]
def sub(tag, data): return tag + struct.pack('<H', len(data)) + data
def plugin(masters):
    body = sub(b'HEDR', struct.pack('<fII', 1.34, 0, 0x800))
    for master in masters:
        body += sub(b'MAST', master.encode() + b'\0') + sub(b'DATA', b'\0' * 8)
    return b'TES4' + struct.pack('<III', len(body), 0, 0) + b'\0' * 8 + body
for name, masters in [('NeedsAbsentDLC.esp', ['FalloutNV.esm', 'LonesomeRoad_absent.esm']),
                      ('PatchForIt.esp', ['FalloutNV.esm', 'NeedsAbsentDLC.esp']),
                      ('OtherCase.esp', ['FALLOUTNV.ESM'])]:
    with open(os.path.join(out, name), 'wb') as f:
        f.write(plugin(masters))
EOF
# A paired archive beside a generated plugin (a real one no profile plugin pairs, so it would open), and a name the New
# Vegas rule rejects. Pairing compares archives by their resolved paths: a link to an archive another plugin brought is
# that archive, already paired.
ln -s "$DATA/CaravanPack - Main.bsa" "$WORK/OtherCase - Assets.bsa"
ln -s "$DATA/CaravanPack - Main.bsa" "$WORK/OtherCase_Assets.bsa"

fail() { echo "smoke[$GATE]: FAIL -- $*" >&2; exit 1; }
run() { # run <leg> <args...>: the engine's combined output in $WORK/<leg>.log, its exit status in $status
    local leg="$1"; shift
    set +e
    "$BIN" "$@" --list-cells NVDLC01 >"$WORK/$leg.log" 2>&1
    status=$?
    set -e
}
expect() { grep -Fq -- "$2" "$WORK/$1.log" || fail "$1: expected '$2' (see $WORK/$1.log)"; }

# The generated plugins live in $WORK, not the Data directory; they are passed by their paths.
run hoist --master "$DATA/DeadMoney.esm" --master "$DATA/FalloutNV.esm" --esm "$WORK/OtherCase.esp"
(( status == 0 )) || fail "hoist: --list-cells exited $status (see $WORK/hoist.log)"
expect hoist "hoisted: FalloutNV.esm now loads ahead of DeadMoney.esm"
expect hoist "Load order: falloutnv.esm → deadmoney.esm → othercase.esp"
expect hoist "NVDLC01Casino"
expect hoist "DeadMoney.esm pairs DeadMoney - Main.bsa, DeadMoney - Sounds.bsa"
expect hoist "OtherCase.esp pairs OtherCase - Assets.bsa"
! grep -Fq "OtherCase_Assets.bsa" "$WORK/hoist.log" || fail "hoist: OtherCase_Assets.bsa was paired (see $WORK/hoist.log)"
echo "smoke[$GATE]: PASS -- a late master is hoisted and the order loads (LMC-ID-003); a master named in another case matches"
echo "smoke[$GATE]: PASS -- an .esm and an .esp bring the archives the New Vegas rule pairs with them (LMC-AST-002)"

run missing --master "$DATA/FalloutNV.esm" --master "$WORK/NeedsAbsentDLC.esp" --master "$WORK/PatchForIt.esp" \
    --master "$WORK/OtherCase.esp" --esm "$DATA/DeadMoney.esm"
(( status == 0 )) || fail "missing: --list-cells exited $status (see $WORK/missing.log)"
expect missing "disabled: NeedsAbsentDLC.esp — its master LonesomeRoad_absent.esm is not in the profile"
expect missing "disabled: PatchForIt.esp — its master is disabled: NeedsAbsentDLC.esp (missing master LonesomeRoad_absent.esm)"
expect missing "Load order: falloutnv.esm → othercase.esp → deadmoney.esm"
expect missing "NVDLC01Casino"
echo "smoke[$GATE]: PASS -- a missing master disables its plugin and its dependent, and the rest loads (LMC-ID-011)"

run refused --esm "$DATA/DeadMoney.esm"
(( status != 0 )) || fail "refused: a session plugin with a missing master loaded (see $WORK/refused.log)"
expect refused "(--esm) cannot be activated"
expect refused "pass \`--master FalloutNV.esm\` before \`--esm\`"
echo "smoke[$GATE]: PASS -- a session plugin missing its own master is refused with the flag to add"

set +e
"$BIN" --master "$DATA/FalloutNV.esm" --esm "$DATA/DeadMoney.esm" --plugin-report 000000D2 fields >"$WORK/report.log" 2>&1
status=$?
set -e
(( status == 0 )) || fail "report: --plugin-report exited $status (see $WORK/report.log)"
expect report "DeadMoney.esm pairs DeadMoney - Main.bsa, DeadMoney - Sounds.bsa"
expect report "carriers in load order: falloutnv.esm → deadmoney.esm (wins)"
expect report "deadmoney.esm over falloutnv.esm: QSTI added, INFC 4 dropped, INFX 4 dropped"
echo "smoke[$GATE]: PASS -- --plugin-report names a Dead Money topic's carriers and the sub-records it changes (LMC-REC-007)"

echo "smoke[$GATE]: PASS"
