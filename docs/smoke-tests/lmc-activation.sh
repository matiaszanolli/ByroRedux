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
#   5. list merge: the generated ListBase.esm defines a levelled list, a form list, an actor and a keyworded activator,
#      and MergeA.esp and MergeB.esp, neither a master of the other, each add an entry to the levelled list, one adds to
#      the form list what the other removes from it, one gives the actor an item and a promotion while the other gives
#      it a spell, and one adds a keyword to the activator while the other drops its every keyword. Under the default
#      `merge` policy every operation stands and the report names who made each; under `--merge-policy legacy`
#      MergeB.esp's records win whole (LMC-REC-003).
#   6. mod roots: three generated roots after New Vegas' Data (`--mod-root`, lowest priority first). ModA and ModB each
#      ship a plugin and loose textures, ModB's shadowing one of ModA's; DLCPatch ships only a plugin whose master is
#      absent. The `--esm` is a bare plugin name that resolves through the roots; the report lists each root's plugins
#      and loose files, ModB's taking a path from ModA (LMC-ID-012, LMC-AST-010), and DLCPatch disabled with its only
#      plugin (LMC-ID-011).
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
# A plugin is a TES4 record with a New Vegas HEDR and its MAST list (activation reads nothing else), and for leg 5 one
# LVLI, one FLST, one NPC_ and one ACTI record, each in its group: forms 01000800 to 01000803, ListBase.esm's own (its
# one master, FalloutNV.esm, is index 00).
python3 -I - "$WORK" <<'EOF'
import os, struct, sys
out = sys.argv[1]
def sub(tag, data): return tag + struct.pack('<H', len(data)) + data
def lvli(entries):
    body = sub(b'EDID', b'LMCMergeList\0') + sub(b'LVLD', b'\0') + sub(b'LVLF', b'\0')
    for level, form, count in entries:
        body += sub(b'LVLO', struct.pack('<HHIHH', level, 0, form, count, 0))
    return group(b'LVLI', 0x01000800, body)
def flst(forms):
    return group(b'FLST', 0x01000801, sub(b'EDID', b'LMCMergeFormList\0') + b''.join(sub(b'LNAM', struct.pack('<I', f)) for f in forms))
def npc(inventory, factions, spells):
    body = sub(b'EDID', b'LMCMergeActor\0')
    body += b''.join(sub(b'SNAM', struct.pack('<IB3x', form, rank)) for form, rank in factions)
    body += b''.join(sub(b'CNTO', struct.pack('<Ii', form, count)) for form, count in inventory)
    body += b''.join(sub(b'SPLO', struct.pack('<I', form)) for form in spells)
    return group(b'NPC_', 0x01000802, body)
def acti(keywords):
    body = sub(b'EDID', b'LMCMergeActivator\0') + sub(b'MODL', b'Clutter\\Lantern01.nif\0')
    if keywords:
        body += sub(b'KSIZ', struct.pack('<I', len(keywords))) + sub(b'KWDA', b''.join(struct.pack('<I', k) for k in keywords))
    return group(b'ACTI', 0x01000803, body)
def group(tag, form, body):
    record = tag + struct.pack('<III', len(body), 0, form) + b'\0' * 8 + body
    return b'GRUP' + struct.pack('<I', 24 + len(record)) + tag + b'\0' * 12 + record
def plugin(masters, records=b'', esm=False):
    body = sub(b'HEDR', struct.pack('<fII', 1.34, 0, 0x800))
    for master in masters:
        body += sub(b'MAST', master.encode() + b'\0') + sub(b'DATA', b'\0' * 8)
    return b'TES4' + struct.pack('<III', len(body), 1 if esm else 0, 0) + b'\0' * 8 + body + records
for name, masters in [('NeedsAbsentDLC.esp', ['FalloutNV.esm', 'LonesomeRoad_absent.esm']),
                      ('PatchForIt.esp', ['FalloutNV.esm', 'NeedsAbsentDLC.esp']),
                      ('OtherCase.esp', ['FALLOUTNV.ESM'])]:
    with open(os.path.join(out, name), 'wb') as f:
        f.write(plugin(masters))
# Entries name FalloutNV.esm forms 00000A01.. (any form will do: the merge compares entries, it does not resolve them).
# The actor: (inventory, factions, spells); then the activator's keywords.
lists = {'ListBase.esm': ([(1, 0xA01, 1)], [0xA01, 0xA02], ([(0xA01, 1)], [(0xA05, 0)], []), [0xA07],
                          ['FalloutNV.esm']),
         'MergeA.esp': ([(1, 0xA01, 1), (5, 0xA02, 1)], [0xA01, 0xA02, 0xA04], ([(0xA01, 1), (0xA02, 1)], [(0xA05, 2)], []),
                        [0xA07, 0xA08], ['FalloutNV.esm', 'ListBase.esm']),
         'MergeB.esp': ([(1, 0xA01, 1), (8, 0xA03, 2)], [0xA02], ([(0xA01, 1)], [(0xA05, 0)], [0xA06]), [],
                        ['FalloutNV.esm', 'ListBase.esm'])}
# Leg 6's mod roots, each a directory as a mod manager deploys one: its plugin and its loose files.
for root, plugin_name, masters, loose in [('ModA', 'ModA.esp', ['FalloutNV.esm'], ['textures/lmc/shared.dds', 'textures/lmc/a.dds']),
                                          ('ModB', 'ModB.esp', ['FalloutNV.esm'], ['textures/lmc/shared.dds']),
                                          ('DLCPatch', 'DLCPatch.esp', ['FalloutNV.esm', 'LonesomeRoad_absent.esm'],
                                           ['textures/lmc/patch.dds'])]:
    for rel in loose:
        path = os.path.join(out, 'roots', root, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, 'wb') as f:
            f.write(root.encode())
    with open(os.path.join(out, 'roots', root, plugin_name), 'wb') as f:
        f.write(plugin(masters))
for name, (entries, forms, actor, keywords, masters) in lists.items():
    with open(os.path.join(out, name), 'wb') as f:
        records = lvli(entries) + flst(forms) + npc(*actor) + acti(keywords)
        f.write(plugin(masters, records, esm=name.endswith('.esm')))
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

merge_report() { # merge_report <leg> <form> <extra args...>: --plugin-report <form> over the generated list plugins
    local leg="$1" form="$2"; shift 2
    set +e
    "$BIN" "$@" --master "$DATA/FalloutNV.esm" --master "$WORK/ListBase.esm" --master "$WORK/MergeA.esp" \
        --esm "$WORK/MergeB.esp" --plugin-report "$form" >"$WORK/$leg.log" 2>&1
    status=$?
    set -e
    (( status == 0 )) || fail "$leg: --plugin-report exited $status (see $WORK/$leg.log)"
}
merge_report merge 01000800
expect merge "carriers in load order: listbase.esm → mergea.esp → mergeb.esp (merged)"
expect merge "merged from: mergea.esp, mergeb.esp; policy: merge"
expect merge "  entry 00000A02: added by mergea.esp"
expect merge "  entry 00000A03: added by mergeb.esp"
merge_report legacy 01000800 --merge-policy legacy
expect legacy "winner: mergeb.esp; policy: legacy"
echo "smoke[$GATE]: PASS -- independent additions to one levelled list both stand under merge, the last wins under legacy (LMC-REC-003)"
merge_report formlist 01000801
expect formlist "carriers in load order: listbase.esm → mergea.esp → mergeb.esp (merged)"
expect formlist "  entry 00000A04: added by mergea.esp"
expect formlist "  entry 00000A01: removed by mergeb.esp"
merge_report formlist-legacy 01000801 --merge-policy legacy
expect formlist-legacy "winner: mergeb.esp; policy: legacy"
echo "smoke[$GATE]: PASS -- one mod's addition to a form list and another's removal from it both stand under merge (LMC-REC-003)"
merge_report actor 01000802
expect actor "carriers in load order: listbase.esm → mergea.esp → mergeb.esp (merged)"
expect actor "  inventory 00000A02: added by mergea.esp"
expect actor "  faction 00000A05: changed by mergea.esp"
expect actor "  spell 00000A06: added by mergeb.esp"
merge_report actor-legacy 01000802 --merge-policy legacy
expect actor-legacy "winner: mergeb.esp; policy: legacy"
echo "smoke[$GATE]: PASS -- one mod's item and promotion and another's spell on one actor all stand under merge (LMC-REC-003)"
merge_report keywords 01000803
expect keywords "carriers in load order: listbase.esm → mergea.esp → mergeb.esp (merged)"
expect keywords "keywords merged from: mergea.esp, mergeb.esp; policy: merge"
expect keywords "  keyword 00000A08: added by mergea.esp"
expect keywords "  keyword 00000A07: removed by mergeb.esp"
merge_report keywords-legacy 01000803 --merge-policy legacy
expect keywords-legacy "winner: mergeb.esp; policy: legacy"
echo "smoke[$GATE]: PASS -- one mod's keyword on an activator and another's dropping its every keyword both stand under merge (LMC-REC-003)"

set +e
"$BIN" --master "$DATA/FalloutNV.esm" --master ModA.esp --master DLCPatch.esp --mod-root "$WORK/roots/ModA" \
    --mod-root "$WORK/roots/DLCPatch" --mod-root "$WORK/roots/ModB" --esm ModB.esp --plugin-report >"$WORK/roots.log" 2>&1
status=$?
set -e
(( status == 0 )) || fail "roots: --plugin-report exited $status (see $WORK/roots.log)"
expect roots "    2  ModB.esp"
expect roots "disabled: DLCPatch.esp — its master LonesomeRoad_absent.esm is not in the profile"
expect roots "disabled root: dlcpatch — activation disabled every plugin of the profile it ships (DLCPatch.esp)"
expect roots "mod roots (4), lowest priority first:"
expect roots "    1  moda: 1 plugin (1 in the profile), 0 archives, 2 loose files"
expect roots "    2  dlcpatch: 1 plugin (1 in the profile), 0 archives; disabled with its plugins, its loose files left out"
expect roots "    3  modb: 1 plugin (1 in the profile), 0 archives, 1 loose file, 1 over moda's"
echo "smoke[$GATE]: PASS -- mod roots own their plugins and loose files, a later root's file wins, and a root loses its loose files with its only plugin (LMC-ID-012, LMC-AST-010, LMC-ID-011)"

echo "smoke[$GATE]: PASS"
