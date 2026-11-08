import re, os, sys, collections
D = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = '/mnt/data/src/gamebyro-redux/docs/data/game-ui/'
CS = 'cs-uesp-wiki'; GK = 'geck-uesp-wiki'; CK = 'ck-uesp-wiki'; FK = 'falloutck-uesp-wiki'
REF = '/mnt/data/src/reference/'
HDR = ['menu','code','movie_or_document','kind','name_source','script_entry_points','notes','source']
COMMON_SRC = 'py/inventory.py; archive listing via crates/bsa/examples/archive_list_all'
def norm(s): return re.sub(r'(menu|[^a-z0-9])', '', s.lower())
def wiki(w, rel): return f'{w}/{rel}'
def page_tags(w, name):
    """origin/category tags of a function page, '' if the page is absent from the dump"""
    p = f'{REF}{w}/(main)/{name}.wiki'
    if not os.path.exists(p): return None
    t = open(p, encoding='utf-8').read()
    m = re.search(r'origin\s*=\s*(\S+)', t)
    cats = re.findall(r'Category:\s*Functions\s*\(([^)]*)\)', t)
    return 'origin=' + (m.group(1) if m else '-') + ('; cat=' + '|'.join(cats) if cats else '')
def ep(w, name, label=None, extra=''):
    tag = page_tags(w, name)
    label = label or name
    if tag is None: return f'{label} [no page in {w} dump{extra}]'
    return f'{label} [{w}/(main)/{name}.wiki{"; " + tag if tag else ""}{extra}]'
def load_inv(g):
    rows = []
    for l in open(f'{D}/inv_{g}.tsv', encoding='utf-8').read().splitlines()[1:]:
        c = (l.split('\t') + [''] * 6)[:6]
        rows.append(dict(path=c[0], kind=c[1], tag=c[2], name=c[3], cls=c[4], inc=[x for x in c[5].split(',') if x]))
    return rows
def included_by(rows):
    cnt = collections.Counter()
    for r in rows:
        for i in r['inc']:
            cnt[i.lower().split('\\')[-1].split('/')[-1]] += 1
    return cnt
EXE = {
 'obl': open('/mnt/data/SteamLibrary/steamapps/common/Oblivion/Oblivion.exe', 'rb').read().lower(),
 'fo3': open('/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Fallout3.exe', 'rb').read().lower(),
 'fnv': open('/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/FalloutNV.exe', 'rb').read().lower(),
}
def exe_ref(g, path):
    b = EXE[g]; p = path.lower().encode()
    short = p[len(b'menus\\'):]
    return any(x in b for x in (p, p.replace(b'\\', b'/'), short, short.replace(b'\\', b'/')))
def doc_rows(g, rows, named, hud_names):
    """named: {menu_name: (code, [doc paths], name_source, eps, notes)}; returns TSV rows"""
    inc = included_by(rows)
    used = set(); out = []
    byp = {r['path']: r for r in rows}
    for menu, (code, docs, src, eps, notes) in named.items():
        for d in docs: used.add(d)
        kind = 'hud' if menu in hud_names else 'top-level menu'
        flags = []
        for d in docs:
            r = byp.get(d)
            if r: flags.append(f"{d.split(chr(92))[-1]}: root <{r['tag']} name=\"{r['name']}\"> class {r['cls'] or '-'}; path in {g} exe: {'yes' if exe_ref(g, d) else 'no'}")
        out.append([menu, code, ';'.join(docs) if docs else 'none (no document found for this name)', kind, src, eps, ' | '.join(filter(None, [notes] + flags)), COMMON_SRC])
    for r in rows:
        p = r['path']
        if p in used: continue
        low = p.lower()
        if low.endswith(('.xml',)):
            base = low.split('\\')[-1]
            nin = inc.get(base, 0)
            if r['tag'] == 'menu' and '\\prefabs\\' not in low:
                kind = 'hud' if r['name'] in hud_names else 'top-level menu'
                menu = r['name']
            else:
                kind = 'prefab/include'; menu = base[:-4]
            stub = ''
            out.append([menu, 'unknown' if kind != 'prefab/include' else '', p, kind, 'archive-only (document root %s name="%s")' % (r['tag'] or '-', r['name']), '', f"class {r['cls'] or '-'}; included by {nin} document(s); path in {g} exe: {'yes' if exe_ref(g, p) else 'no'}" + ('; no wiki name/code found' if kind != 'prefab/include' else ''), COMMON_SRC])
        else:
            out.append([low.split('\\')[-1], '', p, 'font/support', 'archive-only', '', 'non-XML file under menus\\ (build helper, source-control file, notes or data); not loaded as a menu', COMMON_SRC])
    return out
def write(name, rows):
    with open(OUT + name, 'w', encoding='utf-8') as f:
        f.write('\t'.join(HDR) + '\n')
        for r in rows: f.write('\t'.join(x.replace('\t', ' ').replace('\n', ' ') for x in r) + '\n')
    print(name, len(rows))
STUB = {'menus\\birthsign_menu.xml', 'menus\\chargen\\attributes_menu.xml', 'menus\\chargen\\specilization_menu.xml', 'menus\\scroll_menu.xml'}

# ============================ Oblivion ============================
def oblivion():
    rows = load_inv('obl')
    T = f'{CS}/Template/MenuTypes.wiki'
    W = lambda n: ep(CS, n)
    d = lambda *x: [r'menus' + '\\' + y for y in x]
    named = {
     'Message': ('1001', d('message_menu.xml', 'message\\message_menu.xml'), T, '; '.join([W('MessageBox'), W('Message')]), 'two copies of the document (menus\\message_menu.xml and menus\\message\\message_menu.xml); which is live not determined'),
     'Inventory': ('1002', d('main\\inventory_menu.xml'), T, '', 'no script function opens it in the CS dump'),
     'Stats': ('1003', d('main\\stats_menu.xml'), T, '', ''),
     'HUDMain': ('1004', d('main\\hud_main_menu.xml', 'main\\hud_main_menu(2).xml'), T, '', 'hud_main_menu(2).xml is a stray copy'),
     'HUDInfo': ('1005', d('main\\hud_info_menu.xml'), T, '', ''),
     'HUDReticle': ('1006', d('main\\hud_reticle.xml'), T, '', 'document root is <rect name="HUDReticle">, not <menu>'),
     'Loading': ('1007', d('loading_menu.xml', 'loading_bar_ingame_menu.xml', 'loading_ingame_menu.xml'), T, '', 'three LoadingMenu documents (full, bar, ingame variants); selection logic not determined'),
     'Container, Barter': ('1008', d('container_menu.xml'), T, ep(CS, 'Activate', extra='; the page notes Activate on a container opens its inventory dialog'), 'OBSE IsBarterMenuActive (cs-uesp-wiki/(main)/IsBarterMenuActive.wiki, not vanilla) says container and barter menus return the same MenuMode code; barter is the same menu code as the container (CS: IsBarterMenuActive page); there is no separate barter document: menus\\negotiate_menu.xml is code 1025 Negotiate'),
     'Dialog': ('1009', d('dialog\\dialog_menu.xml'), T, ep(CS, 'StartConversation'), ''),
     'HUDSubtitle': ('1010', d('main\\hud_subtitle_menu.xml'), T, '', 'ShowDialogSubtitles (cs-uesp-wiki/(main)/ShowDialogSubtitles.wiki) only switches subtitles on/off globally - it does not open this menu, although the engine queues a "dialog subtitles" MenuRequest for it (obscript_more/state.rs:352)'),
     'Generic': ('1011', d('generic\\quest_added.xml', 'generic\\skill_perk.xml', 'generic\\test_menu.xml'), T, '', 'three documents share class GenericMenu (QuestAddedMenu, SkillPerkMenu, GenericTestMenu)'),
     'SleepWait': ('1012', d('sleep_wait_menu.xml'), T, '', ''),
     'Pause': ('1013', d('options\\pause_menu.xml'), T, '', ''),
     'LockPick': ('1014', d('lockpick_menu.xml'), T, '', ''),
     'Options': ('1015', d('options\\options_menu.xml'), T, '', ''),
     'Quantity': ('1016', d('quantity_menu.xml'), T, '', ''),
     'Audio': ('1017', d('options\\audio_menu.xml'), T, '', ''),
     'Video': ('1018', d('options\\video_menu.xml'), T, '', ''),
     'VideoDisplay': ('1019', d('options\\video_display_menu.xml'), T, '', ''),
     'Gameplay': ('1020', d('options\\gameplay_menu.xml'), T, '', ''),
     'Controls': ('1021', d('options\\controls_menu.xml', 'options\\xcontrols_menu.xml'), T, '', 'xcontrols_menu.xml (root XControlsMenu, class ControlsMenu) is the console-pad variant'),
     'Magic': ('1022', d('main\\magic_menu.xml'), T, '', ''),
     'Map': ('1023', d('main\\map_menu.xml'), T, ep(CS, 'ShowMap', extra='; the page examples say it shows a marker on the world map (optionally enabling fast travel); it does NOT say it opens the map menu'), 'the engine request named "map" (obscript_more.rs:371) is therefore not a documented menu open'),
     'MagicPopup': ('1024', d('main\\magic_popup_menu.xml'), T, '', ''),
     'Negotiate': ('1025', d('negotiate_menu.xml'), T, '', ''),
     'Book': ('1026', d('book_menu.xml'), T, '', ''),
     'LevelUp': ('1027', d('levelup_menu.xml'), T, '', ''),
     'Training': ('1028', d('training_menu.xml'), T, '', ''),
     'BirthSign': ('1029', d('chargen\\birthsign_menu.xml', 'birthsign_menu.xml'), T, ep(CS, 'ShowBirthsignMenu'), 'menus\\birthsign_menu.xml is a 47-byte stub comment ("NOT USED - SEE ..."); menus\\chargen\\birthsign_menu.xml is the live-looking document'),
     'Class': ('1030', d('chargen\\class_menu.xml', 'class_menu.xml'), T, ep(CS, 'ShowClassMenu'), 'two copies (menus\\class_menu.xml and menus\\chargen\\class_menu.xml)'),
     'Attributes': ('1031', d('chargen\\attributes_menu.xml'), T, '', 'document is a 47-byte stub comment ("NOT USED - SEE skills_menu.xml INSTEAD")'),
     'Skills': ('1032', d('chargen\\skills_menu.xml'), T, '', ''),
     'Specialization': ('1033', d('chargen\\specilization_menu.xml'), T, '', 'document is a 47-byte stub comment (NOT USED)'),
     'Persuasion': ('1034', d('persuasion_menu.xml', 'dialog\\persuasion_menu.xml'), T, '', 'two copies'),
     'Repair / Ingredient Selection': ('1035', d('repair_menu.xml', 'ingredient_select_menu.xml'), T, '', 'wiki name lists both; ingredient_select_menu.xml has root name RepairMenu, class IngredientSelectMenu'),
     'RaceSex': ('1036', d('chargen\\race_sex_menu.xml'), T, ep(CS, 'ShowRaceMenu'), 'CS page warns calling more than one character-creation menu at a time freezes controls (GUI-PAGE-002)'),
     'SpellPurchase': ('1037', d('dialog\\spell_purchase.xml'), T, '', ''),
     'Load': ('1038', d('options\\load_menu.xml'), T, '', ''),
     'Save': ('1039', d('options\\save_menu.xml'), T, '', ''),
     'Alchemy': ('1040', d('dialog\\alchemy.xml'), T, '', 'no script function cited'),
     'SpellMaking': ('1041', d('dialog\\spellmaking.xml'), T, ep(CS, 'ShowSpellMaking'), ''),
     'Enchantment': ('1042', d('dialog\\enchantment.xml'), T, ep(CS, 'ShowEnchantment'), ''),
     'EffectSetting': ('1043', d('dialog\\enchantmentsetting_menu.xml'), T, '', 'document root name EffectSettingMenu'),
     'Main': ('1044', d('options\\main_menu.xml', 'options\\copy of main_menu.xml'), T, '', '"copy of main_menu.xml" is a stray copy'),
     'Breath': ('1045', d('breath_meter_menu.xml'), T, '', 'wiki name Breath joined to breath_meter_menu.xml (root BreathMenu)'),
     'QuickKeys': ('1046', d('main\\quickkeys_menu.xml'), T, '', ''),
     'Credits': ('1047', d('options\\credits_menu.xml'), T, '', ''),
     'SigilStone': ('1048', d('dialog\\sigilstone.xml'), T, '', ''),
     'Recharge': ('1049', d('recharge_menu.xml'), T, '', ''),
     'TextEdit': ('1051', d('dialog\\texteditmenu.xml'), T, '', 'OBSE OpenTextInput (cs-uesp-wiki/(main)/OpenTextInput.wiki, not vanilla) opens a text input menu'),
    }
    out = doc_rows('obl', rows, named, {'HUDMain', 'HUDInfo', 'HUDReticle', 'HUDSubtitle', 'HUDMainMenu', 'HUDInfoMenu', 'HUDSubtitleMenu', 'HUDSafeZone', 'Breath'})
    out.append(['(code 1050)', '1050', 'none', 'top-level menu', T, '', 'the CS Template:MenuTypes list jumps from 1049 Recharge to 1051 TextEdit: code 1050 is unnamed in the wiki; unknown', COMMON_SRC])
    # fonts
    b = [l for l in open(f'{D}/obl_misc_all.txt').read().splitlines() if l.lower().startswith('fonts\\') and l.lower().endswith('.fnt')]
    for p in sorted(b): out.append([p.split('\\')[-1], '', p, 'font/support', 'archive-only', '', 'bitmap font loaded by MenuXml <font> slot; atlas is the .tex beside it', COMMON_SRC])
    write('menu-inventory-oblivion.tsv', out)
oblivion()

# ============================ Fallout 3 / New Vegas ============================
GT = f'{GK}/Template/MenuTypes.wiki'
GMM = f'{GK}/(main)/MenuMode.wiki'
def fallout(g):
    rows = load_inv(g)
    fnv = (g == 'fnv')
    W = lambda n, **k: ep(GK, n, **k)
    N = lambda n: f'{n} [name listed in {GK}/(main)/Complete List of Function in Fallout New Vegas.wiki; no page in dump]'
    d = lambda *x: ['menus' + '\\' + y for y in x]
    if not fnv:
        W0 = W
        W = lambda n, **k: ('' if 'egas' in (page_tags(GK, n) or '') else W0(n, **k))
        N = lambda n: ''
    named = {
     'Message': ('1001', d('message_menu.xml'), GT, W('ShowMessage'), ''),
     'Inventory': ('1002', d('main\\inventory_menu.xml'), GT, '', ''),
     'Stats': ('1003', d('main\\stats_menu.xml'), GT, '', ''),
     'HUDMainMenu': ('1004', d('main\\hud_main_menu.xml'), GT, '', 'GECK Template:MenuTypes names 1004 HUDMainMenu; xEdit wbMenuModeEnum names it MainMenu (GUI-REG-006 verify, unresolved here: work package D owns the xNVSE/xEdit side). GECK MenuMode page says the Main Menu "does not have its own menu code" and 1004, 1007, 1013 return true while it is up'),
     'Loading': ('1007', d('loading_menu.xml'), GT, '', ''),
     'Container': ('1008', d('container_menu.xml'), GT, '; '.join([ep(GK, 'Activate', extra='; the page notes Activate on a container opens its inventory dialog'), W('OpenTeammateContainer')]), ''),
     'Dialog': ('1009', d('dialog\\dialog_menu.xml'), GT, W('StartConversation'), ''),
     'SleepWait': ('1012', d('sleep_wait_menu.xml'), GT, W('ShowSleepWaitMenu'), ''),
     'Pause': ('1013', d('options\\pause_menu.xml'), GT, '', ''),
     'LockPick': ('1014', d('lockpick_menu.xml'), GT, W('ShowLockPickMenu') + '; ' + N('ShowLockpickMenuDebug'), ''),
     'Quantity': ('1016', d('quantity_menu.xml'), GT, '', ''),
     'Pipboy Data': ('1023', d('main\\map_menu.xml'), GT, '', 'GECK ShowMap page (geck-uesp-wiki/(main)/ShowMap.wiki) says it adds a map marker to the player world map; it does not say it opens this menu, so it is not listed as an entry point. Join to map_menu.xml is INFERRED: GECK names 1023 "Pipboy Data"; the document is MapMenu and draws &-sData; (the DATA tab). Oblivion code 1023 is Map'),
     'LevelUp': ('1027', d('levelup_menu.xml'), GT, '', ''),
     'Pipboy Repair': ('1035', d('repair_menu.xml'), GT, '', 'join INFERRED by wording (wiki "Pipboy Repair" vs document RepairMenu)'),
     'RaceSex': ('1036', d('chargen\\race_sex_menu.xml'), GT, W('ShowRaceMenu'), 'GECK ShowRaceMenu page: brings up the "Gene Projector" menu (sex, race, face, hair)'),
     'Credits': ('1047', d('options\\credits_menu.xml'), GT, '', ''),
     'CharGen': ('1048', d('chargen\\char_gen_menu.xml'), GT, N('ShowChargenMenu') + '; ' + N('ShowChargenMenuParams') + '; ' + N('ShowChargenMenuModValues'), 'join INFERRED by wording (wiki CharGen vs CharGenMenu)'),
     'TextEdit': ('1051', d('dialog\\texteditmenu.xml'), GT, '', ''),
     'Barter': ('1053', d('barter_menu.xml'), GT, W('ShowBarterMenu'), ''),
     'Surgery': ('1054', d('surgerymenu.xml'), GT, N('ShowSurgeryMenu'), ''),
     'Hacking': ('1055', d('hacking_menu.xml'), GT, N('ShowHackingMiniGame'), ''),
     'VATS': ('1056', d('vats_menu.xml'), GT, '', 'never pauses the world on its own account (GUI-REG-004 / VATS-UI-007); what it shows belongs to the VATS PRD'),
     'Computers': ('1057', d('computers_menu.xml'), GT, N('ShowComputersInterface'), ''),
     'Vendor Repair': ('1058', d('repair_services_menu.xml'), GT, W('ShowRepairMenu'), 'join INFERRED by wording (wiki Vendor Repair vs RepairServicesMenu, whose strings are sRepairServicesTitle / sRepairAllItems)'),
     'Tutorial': ('1059', d('tutorial_menu.xml'), GT, W('ShowTutorialMenu'), ''),
     "You're SPECIAL book": ('1060', d('chargen\\specialbookmenu.xml'), GT, W('ShowSPECIALBookMenuParams') + '; ' + N('ShowSPECIALBookMenu'), ''),
    }
    if fnv:
        named.update({
         'Item Mod Menu': ('1061', d('item_mod_menu.xml'), GT, '', '(New Vegas) per the GECK list'),
         'Love Tester': ('1074', d('chargen\\love_tester_menu.xml'), GT, W('ShowLoveTesterMenuParams') + '; ' + N('ShowLoveTesterMenu'), '(New Vegas); the page calls it the Vigor Tester'),
         'Companion Wheel': ('1075', d('companion_wheel_menu.xml'), GT, '', '(New Vegas)'),
         'The Medical Questionnaire (Not Used)': ('1076', [], GT, '', '(New Vegas) the list says "Not Used"; no document of this name; the character-creation traits menus are 1084'),
         'Recipe': ('1077', d('recipe_menu.xml'), GT, N('ShowRecipeMenu'), '(New Vegas)'),
         'Slot Machine MiniGame': ('1080', d('slot_machine_menu.xml'), GT, N('ShowSlotMachineMenu') + '; ' + N('ShowSlotMachineMenuParams'), '(New Vegas)'),
         'Blackjack Table MiniGame': ('1081', d('black_jack_menu.xml'), GT, N('ShowBlackJackMenu') + '; ' + N('ShowBlackJackMenuParams'), '(New Vegas)'),
         'Roulette Table MiniGame': ('1082', d('roulette_menu.xml'), GT, N('ShowRouletteMenu') + '; ' + N('ShowRouletteMenuParams'), '(New Vegas)'),
         'Caravan MiniGame': ('1083', d('caravan_menu.xml'), GT, W('ShowCaravanMenu'), '(New Vegas); the caravan_*_menu.xml fragments (bet, builddeck, game, results) are rect-rooted pieces listed separately'),
         'Character Creation Traits': ('1084', d('trait_menu.xml', 'trait_select_menu.xml'), GT, N('ShowTraitMenu') + '; ' + N('ShowTraitSelectMenu'), '(New Vegas) join INFERRED by wording; the wiki text says "(not the Medical Questionnaire)"'),
        })
    if not fnv:
        for k, v in list(named.items()):
            e = re.sub(r'(; )+', '; ', v[3]).strip('; ')
            named[k] = (v[0], v[1], v[2], e, v[4])
    out = doc_rows(g, rows, named, {'HUDMainMenu', 'HUDSafeZone', 'BreathMenu'})
    for fn, what in (('ShowBarberMenu', 'brings up the menu which allows the player to select hair color and style'), ('ShowPlasticSurgeonMenu', 'brings up the menu which allows the player to change his or her face'), ('ShowNameMenu', 'displays the menu that allows the player to change her/his name')):
        out.append([f'unknown (opened by {fn})', 'unknown', 'unknown', 'top-level menu', f'{GK}/(main)/{fn}.wiki', ep(GK, fn, extra='; the page does not name the menu or its code'), f'function-only row: the page says it {what}; the menu name, code and document are not stated by any source read here', COMMON_SRC])
    # gaps in the GECK list
    out.append(['(codes 1005, 1006, 1010, 1011, 1015, 1017-1022, 1024-1026, 1028-1034, 1037-1046, 1049, 1050, 1052)', 'unknown', 'none', 'top-level menu', GT, '', 'the GECK Template:MenuTypes list elides these with "..." (and so does MenuMode.wiki); xEdit wbMenuModeEnum / xNVSE constants are work package D\'s. Not sourced here', COMMON_SRC])
    if fnv:
        out.append(['(codes 1062-1073, 1078, 1079)', 'unknown', 'none', 'top-level menu', GT, '', 'elided by the GECK list ("..." gaps); not sourced here', COMMON_SRC])
    else:
        out.append(['(codes marked "(New Vegas)" in the GECK list: 1061, 1074-1077, 1080-1084)', 'n/a', 'none', 'top-level menu', GT, '', 'New Vegas only; no matching FO3 document (Fallout 3 Misc BSA has no item_mod / companion_wheel / recipe / casino / trait documents)', COMMON_SRC])
    fonts = [l for l in open(f'{D}/{g}_misc_all.txt').read().splitlines() if l.lower().startswith('fonts\\')]
    note = 'FO3/FNV fonts are in "Fallout - Misc.bsa" (fonts\\ .fnt/.tex); the MenuProfile reads font slots from the Textures BSA (textures\\fonts\\)' if False else 'font file in the Misc BSA'
    for p in sorted(fonts):
        out.append([p.split('\\')[-1], '', p, 'font/support', 'archive-only', '', note, COMMON_SRC])
    write(f'menu-inventory-{g}.tsv', out)
fallout('fo3'); fallout('fnv')

# ============================ Skyrim SE / Fallout 4 (Scaleform) ============================
SKYEXE = open('/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/SkyrimSE.exe', 'rb').read().lower()
FO4EXE = open('/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Fallout4.exe', 'rb').read().lower()
def movie_note(exe, path, how):
    stem = path.split('\\')[-1].rsplit('.', 1)[0].lower().encode()
    return f"{how}; stem '{path.split(chr(92))[-1].rsplit('.',1)[0]}' {'present' if stem in exe else 'ABSENT'} as a string in the game exe"
def scaleform(game, listing, exe, names, name_src, extra_rows_fn, outname, hud_names, kinds_fn):
    files = [l for l in open(f'{D}/{listing}').read().splitlines()]
    swf_top = sorted(l for l in files if l.lower().endswith('.swf') and l.lower().startswith('interface\\') and l.count('\\') == 1)
    stem = lambda p: p.split('\\')[-1].rsplit('.', 1)[0].lower()
    by_stem = {stem(p): p for p in swf_top}
    out = []; used = set()
    for menu, (movie_stem, eps, src, notes) in names.items():
        if movie_stem is None:
            mov = 'none (no movie of a matching name; unknown which movie draws it)'; mn = ''
        else:
            p = by_stem.get(movie_stem.lower())
            if p is None: raise SystemExit('missing movie ' + movie_stem)
            used.add(p); mov = p
            mn = movie_note(exe, p, 'menu->movie join INFERRED from the file name (the wiki lists names, not files)')
        kind = 'hud' if menu in hud_names else 'top-level menu'
        out.append([menu, 'n/a (name-addressed)', mov, kind, src, eps, ' | '.join(filter(None, [notes, mn])), COMMON_SRC])
    for p in swf_top:
        if p in used: continue
        kind, notes = kinds_fn(p)
        mn = movie_note(exe, p, 'archive-only')
        out.append([stem(p), 'n/a', p, kind, 'archive-only (no wiki name matched)', '', ' | '.join(filter(None, [notes, mn])), COMMON_SRC])
    out += extra_rows_fn(files)
    write(outname, out)

def sky_kinds(p):
    s = p.split('\\')[-1].lower()
    if s.startswith('fonts_') or s == 'gfxfontlib.swf': return 'font/support', 'font library'
    if s in ('sharedcomponents.swf', 'loadwaitspinner.swf', 'widgetoverlay.swf', 'safezone.swf'): return 'prefab/include', 'shared component / overlay support movie'
    if s == 'book.swf': return 'prefab/include', 'book page art used by Book Menu (inferred from the name)'
    return 'top-level menu', 'no menu name found in the CK UI Script list; name above is the file stem'

def sky_extra(files):
    out = []
    for p in sorted(files):
        l = p.lower()
        if l.startswith('interface\\inventory components\\') and l.endswith('.swf'):
            out.append([p.split('\\')[-1][:-4], 'n/a', p, 'prefab/include', 'archive-only', '', 'inventory component movie loaded inside InventoryMenu/ContainerMenu/BarterMenu/GiftMenu/MagicMenu (inferred from the folder name)', COMMON_SRC])
    cnt = collections.Counter(p.lower().rsplit('.', 1)[-1] for p in files if p.lower().startswith('interface\\') and p.count('\\') == 1 and not p.lower().endswith('.swf'))
    out.append(['(Interface\\*.gfx, *.txt, *.csv, *.xlsx, translate_*, fontconfig*)', 'n/a', 'see notes', 'font/support', 'archive-only', '', 'non-movie interface files: ' + ', '.join(f'{k}={v}' for k, v in sorted(cnt.items())) + '; translate_<language>.txt = $KEY<TAB>value (GUI-FONT-004), fontconfig*.txt (GUI-FONT-001)', COMMON_SRC])
    cd = [p for p in files if p.lower().startswith('interface\\controls\\')]
    out.append(['(Interface\\controls\\**)', 'n/a', f'{len(cd)} files', 'font/support', 'archive-only', '', 'control-map / keyboard text files per platform', COMMON_SRC])
    ddscount = sum(1 for p in files if p.lower().startswith('textures\\') or p.lower().endswith('.dds'))
    out.append(['(textures\\interface\\** .dds)', 'n/a', f'{ddscount} files', 'font/support', 'archive-only', '', 'menu art loaded by the movies', COMMON_SRC])
    return out

def skyrim():
    UI = f'{CK}/(main)/UI Script.wiki'
    A = lambda n: ep(CK, n)
    names = {
     'BarterMenu': ('bartermenu', A('ShowBarterMenu - Actor'), UI + ' (Valid Menu Names)', ''),
     'Book Menu': ('bookmenu', '', UI, ''),
     'Console': ('console', '', UI, ''),
     'Console Native UI Menu': (None, '', UI, 'no movie of this name; native console UI'),
     'ContainerMenu': ('containermenu', A('OpenInventory - Actor'), UI, 'Actor.OpenInventory page: opens this actor\'s inventory, teammates only unless forced'),
     'Crafting Menu': ('craftingmenu', '', UI, 'no Papyrus function cited'),
     'Credits Menu': ('creditsmenu', '', UI, ''),
     'Cursor Menu': ('cursormenu', '', UI, ''),
     'Debug Text Menu': (None, '', UI, 'no movie found'),
     'Dialogue Menu': ('dialoguemenu', '', UI, 'no Papyrus function cited'),
     'Fader Menu': ('fadermenu', '', UI, ''),
     'FavoritesMenu': ('favoritesmenu', '', UI, ''),
     'GiftMenu': ('giftmenu', A('ShowGiftMenu - Actor'), UI, ''),
     'HUD Menu': ('hudmenu', '', UI, 'stays registered open while modal menus are up (GUI-REG-003, verify pending)'),
     'InventoryMenu': ('inventorymenu', A('OpenInventory - Actor'), UI, ''),
     'Journal Menu': ('quest_journal', '', UI, ''),
     'Kinect Menu': ('kinectmenu', '', UI, 'platform-feature menu (name only); host methods, if any, belong to GUI-HOST-003 review'),
     'LevelUp Menu': ('levelupmenu', '', UI, ''),
     'Loading Menu': ('loadingmenu', '', UI, ''),
     'Lockpicking Menu': ('lockpickingmenu', '', UI, ''),
     'MagicMenu': ('magicmenu', '', UI, ''),
     'Main Menu': ('startmenu', A('QuitToMainMenu - Game'), UI, 'wiki name "Main Menu" joined to startmenu.swf INFERRED; Game.QuitToMainMenu returns to it'),
     'MapMenu': ('map', '', UI, ''),
     'MessageBoxMenu': ('messagebox', A('Show - Message') + '; ' + A('MessageBox - Debug'), UI, 'wiki name MessageBoxMenu joined to messagebox.swf INFERRED'),
     'Mist Menu': (None, '', UI, 'no movie found'),
     'Overlay Interaction Menu': (None, '', UI, 'no movie found'),
     'Overlay Menu': (None, '', UI, 'no movie found (widgetoverlay.swf may serve it; unverified)'),
     'Quantity Menu': (None, '', UI, 'no quantitymenu movie in Skyrim - Interface.bsa; unknown which movie draws it'),
     'RaceSex Menu': ('racesex_menu', A('ShowRaceMenu - Game') + '; ' + A('ShowLimitedRaceMenu - Game'), UI, 'ShowLimitedRaceMenu requires Skyrim 1.6 per its page and omits name/sex change'),
     'Sleep/Wait Menu': ('sleepwaitmenu', '', UI, ''),
     'StatsMenu': ('statsmenu', '', UI, ''),
     'TitleSequence Menu': ('titles', A('ShowTitleSequenceMenu - Game') + '; ' + A('HideTitleSequenceMenu - Game'), UI, 'wiki name joined to titles.swf INFERRED'),
     'Top Menu': (None, '', UI, 'no movie found'),
     'Training Menu': ('trainingmenu', A('ShowTrainingMenu - Game'), UI, ''),
     'Tutorial Menu': ('tutorialmenu', '', UI, ''),
     'TweenMenu': ('tweenmenu', '', UI, ''),
    }
    scaleform('skyrim', 'sky_if_all.txt', SKYEXE, names, UI, sky_extra, 'menu-inventory-skyrim-se.tsv', {'HUD Menu'}, sky_kinds)
    # extender-created menus
    p = 'menu-inventory-skyrim-se.tsv'
    with open(OUT + p, 'a', encoding='utf-8') as f:
        f.write('\t'.join(['CustomMenu', 'n/a', 'any movie in Interface\\ named at call time', 'top-level menu', f'{UI} (OpenCustomMenu)', f'UI.OpenCustomMenu(swfPath, flag) / CloseCustomMenu [{UI}; SKSE, not vanilla]', 'extender-created menu; the `OpenCustomMenu - UI` page is absent from the dump', COMMON_SRC]) + '\n')
skyrim()

def fo4_kinds(p):
    s = p.split('\\')[-1].lower()
    if s.startswith('fonts_'): return 'font/support', 'font library'
    if s.startswith('pipboy_'): return 'prefab/include', 'Pip-Boy page movie loaded inside PipboyMenu (inferred from the name)'
    if s in ('widgetoverlay.swf', 'bsbuttonhintbar.swf', 'buttonbarmenu.swf', 'swfloadermenu.swf', 'terminalbuttons.swf', 'mapmarkers.swf'): return 'prefab/include', 'shared component / helper movie'
    if s.startswith('fadermenu'): return 'top-level menu', 'aspect-ratio variant of FaderMenu'
    if s.startswith('scopemenu'): return 'top-level menu', 'variant of ScopeMenu'
    return 'top-level menu', 'no menu name found in the FO4 CK Menus template; name above is the file stem'

def fo4_extra(files):
    out = []
    groups = collections.OrderedDict()
    for p in files:
        l = p.lower()
        if l.startswith('interface\\') and l.count('\\') == 1 and l.endswith('.swf'): continue
        parts = l.split('\\')
        key = '\\'.join(parts[:2]) if len(parts) > 2 else parts[0] + '\\*'
        if parts[0] in ('textures', 'strings'): key = parts[0] + '\\*'
        groups.setdefault(key, []).append(l)
    for key, ls in groups.items():
        ext = collections.Counter(x.rsplit('.', 1)[-1] for x in ls)
        kind = 'prefab/include' if key.startswith('interface\\components') or key.startswith('programs') else 'font/support'
        what = {'interface\\components': 'component movies loaded by Pip-Boy / menus (condition clips, vault boys, perks, quest and faction vault boys, magazines)',
                'interface\\marketplace': 'Creation Club / marketplace art and movies (GUI-NG-003 online class)',
                'interface\\exported': 'exported images/gfx used by movies',
                'interface\\*': 'top-level non-swf interface files (fontconfig, credits, falloutdict, translate_*)',
                'programs\\*': 'Pip-Boy holotape game movies (Atomic Command, Grognak, Pipfall, Red Menace, Zeta Invaders, ...) and config XML',
                'programs\\xml': 'holotape game config XML'}.get(key, 'archive content')
        out.append([f'({key}\\**)', 'n/a', f'{len(ls)} files ({", ".join(f"{k}={v}" for k, v in sorted(ext.items()))})', kind, 'archive-only', '', what, COMMON_SRC])
    return out

def fallout4():
    MN = f'{FK}/Template/Menus.wiki'
    A = lambda n: ep(FK, n)
    UIO = f'{FK}/(main)/UI Script.wiki'
    names = {
     'BarterMenu': ('bartermenu', A('ShowBarterMenu - Actor'), MN, ''),
     'BookMenu': ('book', '', MN, 'joined to book.swf INFERRED (there is no bookmenu.swf)'),
     'Console': ('console', '', MN, ''),
     'ConsoleNativeUIMenu': (None, '', MN, 'no movie of this name; native console UI'),
     'ContainerMenu': ('containermenu', A('OpenInventory - Actor'), MN, ''),
     'CookingMenu': (None, '', MN, 'no movie of this name found; unknown which movie draws it'),
     'CreditsMenu': ('creditsmenu', '', MN, ''),
     'CursorMenu': ('cursormenu', '', MN, ''),
     'DialogueMenu': ('dialoguemenu', '', MN, ''),
     'ExamineMenu': ('examinemenu', '', MN, ''),
     'FaderMenu': ('fadermenu', '', MN, 'three aspect variants fadermenu16x10 / 21x9 / 32x9 also exist'),
     'FavoritesMenu': ('favoritesmenu', '', MN, ''),
     'GenericMenu': (None, '', MN, 'no movie of this name found; unknown'),
     'HUDMenu': ('hudmenu', '', MN, ''),
     'LevelUpMenu': ('levelupmenu', '', MN, ''),
     'LoadingMenu': ('loadingmenu', '', MN, ''),
     'LockpickingMenu': ('lockpickingmenu', '', MN, ''),
     'LooksMenu': ('looksmenu', A('ShowRaceMenu - Game'), MN, 'Game.ShowRaceMenu(akMenuTarget, uiMode 0-4, spouses, vendor) "shows the race/sex change menu"; the page does not say which menu name it registers - LooksMenu vs the "RaceMenu" string the vaultexitelevatorscript fixture references (work package E) is unresolved here'),
     'MainMenu': ('mainmenu', A('QuitToMainMenu - Game'), MN, ''),
     'MessageBoxMenu': ('messageboxmenu', A('Show - Message'), MN, ''),
     'MultiActivateMenu': ('multiactivatemenu', '', MN, ''),
     'PauseMenu': (None, '', MN, 'no pausemenu movie; the pause menu may be drawn by programs\\genericpauseassets.swf or the Pip-Boy; unknown'),
     'PipboyMenu': ('pipboymenu', A('ShowPipboyBootSequence - Game') + '; ' + A('ShowOnPipboy - Terminal'), MN, 'ShowOnPipboy page: does not open or close the Pip-Boy for you'),
     'PromptMenu': ('promptmenu', '', MN, ''),
     'ScopeMenu': ('scopemenu', '', MN, 'scopemenuuw.swf is the underwater variant'),
     'SitWaitMenu': ('sitwaitmenu', '', MN, ''),
     'SleepWaitMenu': ('sleepwaitmenu', '', MN, ''),
     'SPECIALMenu': ('specialmenu', A('ShowSPECIALMenu - Game'), MN, ''),
     'TerminalHolotapeMenu': (None, '', MN, 'no movie of this name; unknown'),
     'TerminalMenu': ('terminalmenu', '', MN, ''),
     'VATSMenu': ('vatsmenu', '', MN, 'never pauses the world on its own account (VATS-UI-007)'),
     'VignetteMenu': ('vignettemenu', '', MN, ''),
     'WorkshopMenu': ('workshop', A('StartWorkshop - ObjectReference'), MN, 'wiki name WorkshopMenu joined to workshop.swf INFERRED'),
     'Workshop_CaravanMenu': ('workshop_caravanmenu', A('OpenWorkshopSettlementMenu - ObjectReference'), MN, ''),
    }
    scaleform('fo4', 'fo4_if_all.txt', FO4EXE, names, MN, fo4_extra, 'menu-inventory-fallout4.tsv', {'HUDMenu'}, fo4_kinds)
    with open(OUT + 'menu-inventory-fallout4.tsv', 'a', encoding='utf-8') as f:
        f.write('\t'.join(['(any registered menu name)', 'n/a', 'name-addressed', 'top-level menu', f'{FK}/(main)/OpenMenu - UI.wiki', f'UI.OpenMenu(menuName) / CloseMenu [{FK}/(main)/OpenMenu - UI.wiki; F4SE, not vanilla]; ' + A('RegisterForMenuOpenCloseEvent - ScriptObject'), 'F4SE opens/closes a menu by name; RegisterForMenuOpenCloseEvent is vanilla Papyrus (GUI-EVT-001)', COMMON_SRC]) + '\n')
fallout4()
