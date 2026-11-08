# Generates docs/data/game-ui/native-page-strings.tsv. Line numbers are located by searching the
# repo file for a needle, so they are measured, not typed.
import sys, re, os
D = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lookup import src
ROOT = '/mnt/data/src/gamebyro-redux/'
fo3exe = dict(src['fo3_exe']); sky = dict(src['sky_tr']); fo4 = dict(src['fo4_tr'])
fnv_esm = dict(src['fnv_esm']); fo3_esm = dict(src['fo3_esm'])
fnvexe = open('/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/FalloutNV.exe', 'rb').read()
def cand(spec):
    out = []
    for item in re.split(r'\s+(?=[gsf]:)', spec.strip()):
        kind, key = item.split(':', 1)
        if kind == 'g':
            v = fo3_esm.get(key) or fnv_esm.get(key) or fo3exe.get(key)
            if v is None: raise SystemExit('missing GMST ' + key)
            in_fnv = (b'\0' + key.encode() + b'\0') in fnvexe or key in fnv_esm
            out.append(f'GMST {key}="{v}" (FO3 exe/ESM value; name {"present" if in_fnv else "ABSENT"} in FNV)')
        elif kind == 's':
            if key not in sky: raise SystemExit('missing sky ' + key)
            out.append(f'Skyrim {key}="{sky[key]}"')
        elif kind == 'f':
            if key not in fo4: raise SystemExit('missing fo4 ' + key)
            out.append(f'FO4 {key}="{fo4[key]}"')
    return '; '.join(out)
filecache = {}
def locate(path, needle, nth=1):
    if path not in filecache:
        filecache[path] = open(ROOT + path, encoding='utf-8').read().split('\n')
    lines = filecache[path]; n = 0
    for i, l in enumerate(lines):
        if needle in l:
            n += 1
            if n == nth: return i + 1
    raise SystemExit(f'needle not found: {path}: {needle!r} #{nth}')
rows = []
def R(page, path, needle, string, disp, reason, spec='', nth=1):
    rows.append((f'{path}:{locate(path, needle, nth)}', string, page, disp, reason, cand(spec) if spec else ''))
E = 'engine-text'; C = 'candidate-for-game-string'
GM = 'crates/debug-ui/src/panels/game_menu.rs'
HUD = 'crates/debug-ui/src/panels/hud.rs'
# ---- window titles
for page, t in (('pause','Paused'),('settings','Settings'),('inventory','Inventory'),('dialogue','Conversation'),('message box','Message'),('barter','Barter'),('crafting','Crafting')):
    R(page, GM, f'=> "{t}"', t, E, 'egui window name only: the window is built with title_bar(false), so this string is never drawn (it is the window id)')
# ---- pause page
R('pause', GM, '"BYROREDUX"', 'BYROREDUX', E, 'engine brand mark; no vanilla equivalent (vanilla pause shows the game logo/art, not text)')
R('pause', GM, 'RichText::new("Paused")', 'Paused', C, 'vanilla pause menu has no caption; nearest vanilla word is the HUD pause hint', 'g:sPauseText f:Pause'.replace('f:Pause','f:$Pause'))
R('pause', GM, 'wide_button(ui, "Continue")', 'Continue', C, 'vanilla pause-menu entry', 'g:sContinue s:$CONTINUE f:$CONTINUE')
R('pause', GM, 'wide_button(ui, "Settings")', 'Settings', C, 'vanilla main/pause-menu entry', 'g:sSettings s:$SETTINGS f:$Settings')
R('pause', GM, 'wide_button(ui, "Inventory")', 'Inventory', C, 'vanilla has no pause-menu inventory entry (inventory is a Pip-Boy/Tween tab); nearest word is the container title', 'g:sContainerItemsTitle f:$INVENTORY')
R('pause', GM, 'wide_button(ui, "Quicksave")', 'Quicksave', C, 'vanilla action name', 'g:sMenuDisplayQuicksaveName s:$Quicksave f:$Quicksave')
R('pause', GM, 'wide_button(ui, "Quickload")', 'Quickload', C, 'vanilla action name; FO3/FNV only have the progress text sQuickLoading ("Quickloading...")', 'g:sQuickLoading s:$Quickload f:$Quickload')
R('pause', GM, 'wide_button(ui, "Quit to desktop")', 'Quit to desktop', C, 'vanilla quit entry; FO3/FNV say Exit Game / Quit', 'g:sExitGameAffirm g:sQuit f:$QUIT s:$QUIT')
R('pause', GM, '"Esc  Continue"', 'Esc  Continue', E, 'key hint naming the PC key (Esc); vanilla hints are art/button glyphs')
# ---- settings page
R('settings', GM, '"← Back"', '← Back', C, 'back button (glyph + word)', 'g:sBack s:$Back f:$Back', nth=1)
R('settings', GM, 'ui.heading("Settings")', 'Settings', C, 'page heading', 'g:sSettings s:$SETTINGS f:$Settings')
R('settings', GM, '"Changes are saved automatically"', 'Changes are saved automatically', E, 'no vanilla equivalent: engine settings page persists on change (settings-io), vanilla applies on exit')
R('settings', GM, '"No settings are registered yet."', 'No settings are registered yet.', E, 'empty-state of the engine settings registry; no vanilla equivalent')
R('settings', GM, 'small_button("Reset")', 'Reset', C, 'per-setting reset-to-default', 'g:sReset f:$Reset')
for sec, spec in (('Gameplay','g:sGameplay s:$Gameplay f:$Gameplay'),('Controls','g:sControls s:$Controls f:$Controls'),('Interface',''),('Rendering',''),('Audio','g:sAudio s:$Audio f:$Audio')):
    R('settings', GM, f'"{sec}" => ', sec, C if spec else E, 'settings section tab (rank table, names come from SettingEntry.section)' + ('' if spec else '; no vanilla equivalent section'), spec)
# ---- settings registry (labels drawn by draw_player_setting)
CORE = 'crates/core/src/settings/builtin.rs'
for lab, spec, why in (('Field of view','', 'engine setting; vanilla stores fDefaultFOV in the INI only'),
    ('UI scale','', 'engine setting; no vanilla equivalent'),
    ('Show crosshair','', 'engine setting; vanilla crosshair is per-HUD-element in the movie/XML'),
    ('Show interaction prompts','', 'engine setting; no vanilla equivalent'),
    ('Show vitals bars','', 'engine setting for the native vitals layer; no vanilla equivalent'),
    ('Show objective text','', 'engine setting for the native objective layer; no vanilla equivalent'),
    ('Upscaler','', 'renderer setting; no vanilla equivalent'),
    ('Terrain height-blend band','', 'renderer setting; no vanilla equivalent'),
    ('Terrain parallax depth','', 'renderer setting; no vanilla equivalent'),
    ('Terrain parallax range','', 'renderer setting; no vanilla equivalent'),
    ('Terrain distance softening','', 'renderer setting; no vanilla equivalent'),
    ('Texture streaming','', 'renderer setting; no vanilla equivalent'),
    ('Texture memory budget','', 'renderer setting; no vanilla equivalent')):
    R('settings', CORE, f'"{lab}"', lab, E, why + ' (label registered via SettingEntry; description string beside it is engine text too)')
IN = 'byroredux/src/interaction.rs'
R('settings', IN, '"Mouse sensitivity"', 'Mouse sensitivity', E, 'engine control setting; vanilla stores fMouseCursorSpeed/fMouseHeadingSensitivity in the INI, menu text lives in the exe/XML (not looked up here)')
R('settings', IN, '"Invert vertical look"', 'Invert vertical look', E, 'engine control setting; no GMST/key found with this wording')
for lab in ('Move forward','Move backward','Strafe left','Strafe right','Jump / ascend','Sprint / boost','Block','Pause'):
    R('settings', IN, f'Self::{ {"Move forward":"MoveForward","Move backward":"MoveBackward","Strafe left":"StrafeLeft","Strafe right":"StrafeRight","Jump / ascend":"Jump","Sprint / boost":"Sprint","Block":"Block","Pause":"Pause"}[lab] } => "{lab}"', lab, E, 'keybinding row label (InputAction::label); vanilla control names live in the exe/XML and were not matched here (unknown)')
R('settings', IN, 'Self::Activate => "Activate"', 'Activate', C, 'keybinding row label', 's:$Activate f:$Activate')
R('settings', IN, 'Self::Inventory => "Inventory"', 'Inventory', C, 'keybinding row label', 'g:sContainerItemsTitle f:$INVENTORY')
R('settings', IN, 'Self::Quicksave => "Quicksave"', 'Quicksave', C, 'keybinding row label', 'g:sMenuDisplayQuicksaveName s:$Quicksave f:$Quicksave')
R('settings', IN, 'Self::Quickload => "Quickload"', 'Quickload', C, 'keybinding row label', 's:$Quickload f:$Quickload')
R('settings', IN, 'Self::Vats => "VATS"', 'VATS', E, 'proper noun; the VATS name is a vanilla UI term but no GMST/key with exactly this value was searched for beyond the value index (none found)')
R('settings', IN, 'Self::Attack => "Attack"', 'Attack', E, 'keybinding row label; no exact-value GMST/key found')
R('settings', IN, '"Selecting a key already in use swaps the two bindings."', 'Selecting a key already in use swaps the two bindings.', E, 'description under every keybinding row; engine rebind policy, no vanilla equivalent')
for lab in ('TAA (native resolution)','FSR 3.1 — Native AA','FSR 3.1 — Quality','FSR 3.1 — Balanced','FSR 3.1 — Performance'):
    R('settings', CORE, f'"{lab}"', lab, E, 'upscaler choice label (SettingChoice); product names, no vanilla equivalent')
R('settings', 'crates/debug-ui/src/panels/studio.rs', '"Invalid setting"', 'Invalid setting', E, 'shared draw_setting_control error (studio.rs, also used by the player settings page); developer-facing')
R('settings', IN, '"×",', '×', E, 'slider unit glyph for mouse sensitivity')
R('settings', IN, 'fn key_label', '(key names via key_label: W, A, S, D, E, ...)', E, 'physical key names for the binding choices; device vocabulary, not game text. Lines of the table are in key_label()')
# ---- inventory page
R('inventory', GM, '"← Back"', '← Back', C, 'back button (inventory page)', 'g:sBack s:$Back f:$Back', nth=2)
R('inventory', GM, 'ui.heading("Inventory")', 'Inventory', C, 'page heading', 'g:sContainerItemsTitle f:$INVENTORY')
R('inventory', GM, '"Tab / Esc  Close"', 'Tab / Esc  Close', E, 'key hint naming PC keys (Tab, Esc)')
R('inventory', GM, '"Inventory unavailable"', 'Inventory unavailable', E, 'engine empty-state (no player/cell); no vanilla equivalent')
R('inventory', GM, '"Enter character mode in a loaded game cell', 'Enter character mode in a loaded game cell to use the player inventory.', E, 'developer-facing hint (character mode is an engine flag); no vanilla equivalent')
R('inventory', GM, '"Your inventory is empty"', 'Your inventory is empty', E, 'empty-state; closest vanilla is sEmpty="[EMPTY]" (list placeholder)', 'g:sEmpty')
R('inventory', GM, '"All",', 'All', C, 'category tab (all items)', 'f:$All')
R('inventory', GM, '{} of {} stacks  ·  {:.1} total weight', '{} of {} stacks  ·  {:.1} total weight', E, 'status line format; "stacks" is engine vocabulary. Words: weight', 'g:sWeight s:$WEIGHT f:$wt')
R('inventory', GM, 'columns[1].heading(&item.name)', '(item.name)', E, 'passes record FULL name through - not an engine string (listed for completeness)')
R('inventory', GM, 'ui.label("Count")', 'Count', C, 'detail row; FO3/FNV have the QTY/Quantity words', 'g:sQuantity g:sInventoryQuantity f:$QUANTITY')
R('inventory', GM, 'ui.label("Weight")', 'Weight', C, 'detail row', 'g:sWeight s:$WEIGHT f:$wt')
R('inventory', GM, 'ui.label("Value")', 'Value', C, 'detail row', 'g:sValue s:$VALUE f:$val')
R('inventory', GM, 'ui.label("Form ID")', 'Form ID', E, 'developer detail; no player-facing vanilla equivalent (consider removing from the player page)')
R('inventory', GM, 'egui::Button::new("Drop")', 'Drop', C, 'item action', 'g:sInventoryDrop s:$Drop f:$DROP')
R('inventory', GM, 'egui::Button::new("Use")', 'Use', C, 'item action', 'g:sInventoryUse s:$Use')
R('inventory', GM, 'if item.equipped { "Unequip" }', 'Unequip', C, 'item action', 'g:sInventoryUnequip f:$UNEQUIP')
R('inventory', GM, '"Equip" };', 'Equip', C, 'item action', 'g:sInventoryEquip s:$Equip f:$EQUIP')
R('inventory', GM, '"Use unavailable"', 'Use unavailable', E, 'engine capability notice (effect not modelled); no vanilla equivalent')
R('inventory', GM, '"Equip unavailable"', 'Equip unavailable', E, 'engine capability notice; no vanilla equivalent')
R('inventory', GM, '"This item\'s effects are not fully supported yet."', "This item's effects are not fully supported yet.", E, 'engine capability notice; no vanilla equivalent')
R('inventory', GM, '"This item type has no runtime equipment contract yet."', 'This item type has no runtime equipment contract yet.', E, 'developer-facing capability notice (jargon); no vanilla equivalent')
R('inventory', GM, '"  ◆"', '  ◆', E, 'equipped marker glyph; vanilla uses art')
R('inventory', GM, '{} · {:08X}', '{category} · {form_id:08X}', E, 'hover text exposing the form id; developer detail')
# inventory category and kind strings (catalog.rs)
CAT = 'byroredux/src/inventory/catalog.rs'
CATN={'Armor':'"Armor",','Weapon':'"Weapon",','Ammo':'("Ammo",','Aid':'("Aid",','Misc':'("Misc",','Junk':'("Junk",','Mods':'("Mods",','Note':'("Note",','Book':'("Book",','Scroll':'("Scroll",','Light':'"Light",','Apparatus':'"Apparatus",','Ingredient':'("Ingredient",','Key':'("Key",'}
for cat, spec in (('Armor','s:$ARMOR'),('Weapon','g:sInventoryWeapons s:$Weapons f:$InventoryCategoryWeapons'),('Ammo','g:sInventoryAmmoTab f:$InventoryCategoryAmmo'),('Aid','g:sInventoryAid f:$InventoryCategoryAid'),('Misc','g:sInventoryMisc f:$InventoryCategoryMisc'),('Junk','g:sJunk f:$InventoryCategoryJunk'),('Mods','f:$InventoryCategoryMods'),('Note','g:sNotesTabText')):
    R('inventory', CAT, CATN[cat], cat, C, 'item category tab/label (describe_kind); the exact word differs by game (Apparel/Armor, Weapons/Weapon)', spec)
for cat in ('Book','Scroll','Light','Apparatus','Ingredient','Key'):
    R('inventory', CAT, CATN[cat], cat, E, 'item category label; no exact-value GMST/key found in the FO3 exe table, Skyrim or FO4 translate files (unknown source: Skyrim/Oblivion names come from the record type)')
for needle, t in (('"Consumable"','Consumable'),('"Loose object modification"','Loose object modification'),('"Scrappable components"','Scrappable components'),('"Miscellaneous item"','Miscellaneous item'),('"Armor rating {:.1}"'.replace('"',''),'Armor rating {n}'),('DT {:.1} · DR {}','DT {n} · DR {m}'),('"Damage {damage}"'.replace('"',''),'Damage {n}'),('Scroll (casting unavailable)','Scroll (casting unavailable)'),('Alchemy apparatus (crafting unavailable)','Alchemy apparatus (crafting unavailable)'),('Carried light (equipping unavailable)','Carried light (equipping unavailable)')):
    R('inventory', CAT, needle, t, E, 'item-detail line built by the engine (describe_kind); stat words (Armor rating, DT, DR, Damage) have vanilla stat labels but the composed line has no vanilla form')
AC = 'byroredux/src/inventory/actions.rs'
R('inventory', AC, '"Unknown"', 'Unknown', E, 'fallback category when the catalog has no entry; developer-facing')
R('inventory', AC, '"Item {:08X}"', 'Item {form_id:08X}', E, 'fallback name when the catalog lacks the item; developer-facing')
R('inventory', AC, '"{} | {improved}"', '{details} | {improved}', E, 'engine-composed detail suffix')
# ---- message box
R('message box', GM, 'wide_button(ui, "Ok")', 'Ok', C, 'default button of a box with no authored buttons (site 2)', 'g:sOk g:sOKText s:$OK f:$OK', nth=2)
R('message box', GM, 'wide_button(ui, "Ok")', 'Ok', C, 'default button of a box with no authored buttons (twice in the file)', 'g:sOk g:sOKText s:$OK f:$OK')
# ---- barter
R('barter', GM, '"The merchant has nothing to trade."', 'The merchant has nothing to trade.', E, 'empty-state; no exact vanilla string found')
R('barter', GM, '"Merchant\'s goods (buy)"', "Merchant's goods (buy)", C, 'column title; vanilla barter lists use BUY/SELL tabs', 'g:sBuy s:$BUY f:$BUY')
R('barter', GM, '"Your goods (sell)"', 'Your goods (sell)', C, 'column title', 'g:sSell s:$Sell f:$SELL')
R('barter', GM, '{title} — gold {gold}', '{title} — gold {n}', C, 'hard-coded currency word "gold" on every game; FO3/FNV currency is Caps, FO4 Caps, Skyrim Gold - must come from the game', 'g:sGold g:sInventoryCaps s:$Gold f:$CAPS')
R('barter', GM, '"Nothing."', 'Nothing.', E, 'empty column; closest vanilla sNone="None" / sEmpty="[EMPTY]"', 'g:sNone g:sEmpty f:$None')
R('barter', GM, '{}  ×{}  —  {} gold', '{name}  ×{count}  —  {price} gold', C, 'row format with hard-coded currency word "gold" (see above)', 'g:sGold')
BA = 'byroredux/src/barter.rs'
R('barter', BA, '"Merchant"', 'Merchant', E, 'fallback vendor heading when the NPC has no authored name')
R('barter', BA, '"You don\'t have enough gold."', "You don't have enough gold.", C, 'barter refusal shown as a player notification; wording hard-codes gold', 'g:sNotEnoughCaps f:$VendorCapsTooLow')
R('barter', BA, '"The merchant doesn\'t have enough gold."', "The merchant doesn't have enough gold.", C, 'barter refusal; wording hard-codes gold', 'g:sMerchantNoGold f:$VendorCapsTooLow')
for t in ('the merchant has none left', "the merchant won't buy that", 'no barter in progress', 'no player', 'not a vendor', 'this game has no barter rules', 'merchant container not placed', 'merchant container outside the load order', 'no record index', 'merchant container has no CONT record'):
    R('barter', BA, f'"{t}"', t, E, 'refusal reason returned as Err and pushed to the player notification layer or the console; engine/developer wording, lower-case; no vanilla equivalent' if t.startswith(('the merchant', 'no barter')) else 'developer diagnostic that reaches begin-barter callers (script ShowBarterMenu) - console/log, rarely the HUD')
# ---- crafting
R('crafting', GM, '"There is nothing to craft here."', 'There is nothing to craft here.', E, 'empty-state; no exact vanilla string')
R('crafting', GM, '"No recipes."', 'No recipes.', E, 'empty-state; no exact vanilla string')
R('crafting', GM, 'egui::Button::new("Craft")', 'Craft', C, 'recipe button', 'f:$CRAFT s:$CRAFTING')
CR = 'byroredux/src/crafting.rs'
R('crafting', CR, '"Crafting"', 'Crafting', E, 'bench-name prefix stripped from the furniture editor id; the heading is DERIVED from an editor id by bench_display_name, not from the furniture FULL name - candidate for the furniture record name', 's:$CRAFTING')
R('crafting', CR, '"Improve {name}: {} -> {}"', 'Improve {name}: {from} -> {to}', E, 'Skyrim smithing improve row; composed text, "->" arrow')
R('crafting', CR, '"unimproved"', 'unimproved', E, 'quality name placeholder')
R('crafting', CR, '(no better quality at your Smithing level)', '{name} (no better quality at your Smithing level)', E, 'Skyrim improve row, disabled')
R('crafting', CR, '"Crafted {name} ({})"', 'Crafted {name} ({n})', E, 'player notification')
R('crafting', CR, '"Crafted {name}"', 'Crafted {name}', E, 'player notification')
R('crafting', CR, '"Improved {} to {}"', 'Improved {item} to {quality}', E, 'player notification')
for t in ('you cannot craft now','no player','no recipes are loaded','this bench does not make that','you cannot improve that yet','you cannot craft that yet','the crafted item could not be added','nothing to improve at your Smithing level','the item changed while improving','no inventory','the item is gone','no item instance pool','this bench offers no recipes','no crafting session slot','no bench is open'):
    R('crafting', CR, f'"{t}"', t, E, 'refusal reason returned as Err and pushed to the player notification layer; lower-case engine wording; no vanilla equivalent')
R('crafting', CR, '"not enough {} ({} of {need})"'.replace('"',''), 'not enough {item} ({have} of {need})', E, 'refusal reason (player notification)')
# ---- dialogue
R('dialogue', GM, '"(speaking)"', '(speaking)', E, 'speaking cue; the vanilla dialogue menu shows no text cue (the NPC lip-syncs)')
R('dialogue', GM, '"(speaking, no voice)"', '(speaking, no voice)', E, 'speaking cue for a line with no voice file; developer-facing')
R('dialogue', GM, '"Replies" } else { "Topics" }', 'Replies / Topics', E, 'list captions; vanilla dialogue menu shows no list captions')
R('dialogue', GM, '"  (said)"', '  (said)', E, 'said-marker; vanilla greys topics already spoken (art), no text')
R('dialogue', GM, 'format!("> {text}")', '> {text}', E, 'selected-topic arrow glyph')
R('dialogue', GM, '"{}. {}"', '{n}. {reply}', E, 'reply numbering (Skyrim/FO3 show numbered replies in the NPC choice UI; layout only)')
R('dialogue', GM, '"The conversation has moved on."', 'The conversation has moved on.', E, 'stale-selection notice; no vanilla equivalent')
R('dialogue', GM, 'egui::Button::new("Close (Esc)")', 'Close (Esc)', C, 'shared close button, dialogue stale-selection branch', 'f:$CLOSE g:sExit s:$Exit f:$Exit', nth=2)
for _n in (1,2,3,4):
    R('crafting/barter', GM, 'wide_button(ui, "Close (Esc)")', 'Close (Esc)', C, 'shared close button (crafting empty, barter empty, barter/crafting main)', 'f:$CLOSE g:sExit s:$Exit f:$Exit', nth=_n)
R('dialogue', GM, 'egui::Button::new("Close (Esc)")', 'Close (Esc)', C, 'shared close button used on every page (7 sites: crafting, barter x2, dialogue x2, plus wide_button); Esc hint is PC key text', 'f:$CLOSE g:sExit s:$Exit f:$Exit')
# ---- HUD
HI = 'byroredux/src/interaction.rs'
for verb, needle, nth, spec, note in (('Activate','Self::Activate => "Activate"',2,'s:$Activate f:$Activate',''),('Open','Self::Door => "Open"',1,'', 'door verb; no exact-value GMST/key found'),('Take all','=> "Take all"',1,'g:sTakeAll s:$Take All f:$TAKE ALL',''),('Take','Self::Pickup => "Take"',1,'s:$Take f:$TAKE',''),('Talk','Self::Npc => "Talk"',1,'f:$TALK g:sVampireTalk','FO3/FNV sVampireTalk is a vampire-feed verb; same word, different context'),('Sleep','Self::Sleep => "Sleep"',1,'g:sSleep','sSleep value is lower-case "sleep"'),('Craft','Self::Craft => "Craft"',1,'f:$Craft','')):
    R('hud prompt', HI, needle, verb, C if spec else E, 'interaction prompt verb (InteractionKind::verb)' + (('; ' + note) if note else ''), spec, nth=nth)
R('hud prompt', HUD, '"[{}] {}"', '[{binding}] {verb}', E, 'prompt format; binding label is a PC key name (ActionBindings::binding_label, default "E")')
R('hud prompt', 'byroredux/src/main.rs', 'unwrap_or("E")', 'E', E, 'default key label')
PR = 'crates/core/src/character/profile.rs'
R('hud vitals', PR, '("Health", "Health"), ("Magicka", "Magicka"), ("Fatigue", "Fatigue")', 'Health', C, 'Oblivion/Skyrim-profile bar label', 'g:sHealth s:$Health')
R('hud vitals', PR, '("Health", "Health"), ("Magicka", "Magicka"), ("Fatigue", "Fatigue")', 'Magicka', C, 'bar label', 's:$Magicka')
R('hud vitals', PR, '("Health", "Health"), ("Magicka", "Magicka"), ("Fatigue", "Fatigue")', 'Fatigue', E, 'bar label; no exact-value GMST/key found (Oblivion names it Fatigue in its own strings, not searched)')
R('hud vitals', PR, '("HP", "Health"), ("AP", "ActionPoints")', 'HP', C, 'FO3/FNV bar label', 'g:sHitPointsShort g:sStatsHP f:$HP')
R('hud vitals', PR, '("HP", "Health"), ("AP", "ActionPoints")', 'AP', C, 'FO3/FNV bar label', 'g:sActionPointsShort g:sStatsAP f:$AP')
R('hud vitals', PR, 'radiation_gauge: Some(("RAD", "RadiationRads"))', 'RAD', C, 'FO3/FNV rad-gauge label', 'g:sStatsRADAbbrev g:sHUDRads')
R('hud vitals', HUD, '"{}  {:.0}/{:.0}"', '{label}  {cur}/{max}', E, 'numeric readout format')
R('hud vitals', HUD, '"magicka" =>', '(label vocabulary: magicka, stamina, fatigue, hp, health, ap, actionpoints, o2, rad, rads)', E, 'vital_color keys on the display label string - presentation logic coupled to engine label spelling, not drawn text')
R('hud objectives', 'byroredux/src/objectives.rs', '"New objective — {subject}"', 'New objective — {quest}: {text}', C, 'objective notification (HUD player-message layer)', 'g:sCurrentObjective')
R('hud objectives', 'byroredux/src/objectives.rs', '"Objective completed — {subject}"', 'Objective completed — {quest}: {text}', E, 'notification; no exact vanilla string found')
R('hud objectives', 'byroredux/src/objectives.rs', '"Objective failed — {subject}"', 'Objective failed — {quest}: {text}', E, 'notification; no exact vanilla string found')
R('hud objectives', 'byroredux/src/objectives.rs', 'format!("objective {}", event.objective)', 'objective {index}', E, 'fallback when the quest record authors no objective text')
# player messages (HUD layer)
CB = 'byroredux/src/combat.rs'
R('hud message', CB, '"+{xp} XP"', '+{xp} XP', C, 'XP gain notice; vanilla shows "Experience Points Earned"/XP', 'g:sStatsXP g:sRewardXP')
R('hud message', CB, '"Level up! You reached level {level}"', 'Level up! You reached level {level}', C, 'level-up notice; vanilla has LEVEL UP / WELCOME TO LEVEL %d', 'g:sLevelUp g:sLevelUpTitleText')
R('hud message', CB, '"+{:.0} skill points ({skill_points} unspent)"', '+{n} skill points ({m} unspent)', C, 'level-up notice', 'g:sLevelUpSkillCounterPl')
R('hud message', CB, '"+{} perk point(s) ({perk_points} unspent)"', '+{n} perk point(s) ({m} unspent)', C, 'level-up notice', 'g:sLevelUpPerkCounterPl')
R('hud message', CB, 'Health/Magicka/Stamina pick(s)', '+{n} Health/Magicka/Stamina pick(s) ({m} unspent)', E, 'Skyrim-style level-up pick notice; no exact vanilla string')
CM = 'byroredux/src/crime.rs'
R('hud message', CM, '{amount} Gold added to {}', '{n} Gold added to {faction|your bounty}', E, 'bounty notice with hard-coded Gold; no exact string')
R('hud message', CM, 'You were seen {doing}. Your bounty is now {total} gold', 'You were seen {doing}. Your bounty is now {total} gold', E, 'bounty notice; no exact string (hard-codes gold)')
R('hud message', CM, '"stealing"', 'stealing', C, 'crime noun in the bounty notice', 'g:sCrimeTypeSteal')
R('hud message', CM, '"committing murder"', 'committing murder', E, 'crime noun')
R('hud message', CM, '"assaulting someone"', 'assaulting someone', E, 'crime noun')
R('hud message', CM, '"You have escaped from jail"', 'You have escaped from jail', E, 'jail notice; no exact string')
R('hud message', CM, '"You have served your sentence"', 'You have served your sentence', E, 'jail notice; no exact string (vanilla has sNoWaitInJail for the sleep requirement)', 'g:sNoWaitInJail')
R('hud message', HI, '"Picked the lock"', 'Picked the lock', E, 'lock notice; no exact string')
R('hud message', HI, '"Hacked the terminal"', 'Hacked the terminal', E, 'terminal notice; no exact string')
R('hud message', HI, '"Unlocked with key"', 'Unlocked with key', C, 'key notice', 'g:sOpenWithKey')
R('hud message', AC, '"Used {name}"', 'Used {name}', E, 'consume notice; no exact string (vanilla uses effect text)')
R('hud message', AC, '"Dropped {name}"', 'Dropped {name}', E, 'drop notice; no exact string')
R('hud message', AC, '"Can\'t {verb} {} now"', "Can't {use|drop|equip} {name} now", C, 'refusal notice; vanilla has per-case strings', 'g:sCantEquipGeneric g:sDropEquippedItemWarning g:sDropQuestItemWarning')
LT = 'byroredux/src/inventory/loot.rs'
R('hud message', LT, '"Stolen {} {noun}"', 'Stolen {n} item(s)', E, 'container-take notice; no exact string')
R('hud message', LT, '"Took {} {noun}"', 'Took {n} item(s)', E, 'container-take notice; vanilla shows an item-added line', 'g:sAddItemtoInventory'.replace('g:sAddItemtoInventory',''))
R('hud message', LT, '"Stolen {name}"', 'Stolen {name}', C, 'pickup notice', 's:$Stolen')
R('hud message', LT, '"Added {name}"', 'Added {name}', C, 'pickup notice; vanilla appends sAddItemtoInventory ("added")', 'g:sAddItemtoInventory')
R('hud message', LT, '"item" } else { "items"'.replace('"item" } else { "items"','"item"'), 'item / items', E, 'plural noun in the take notice')
# save / load notices (first output line becomes the player message)
SC = 'byroredux/src/save_io/commands.rs'
R('hud message', SC, 'format!("saved slot {slot} → {}", path.display())', 'saved slot {n} → {path}', C, 'quicksave success shown as the player message; exposes a filesystem path; vanilla shows "Quicksaving..." text', 'g:sQuickSaving s:$Quicksave')
R('hud message', SC, '"queued load of slot {slot} → {destination_label} (applies next frame)"'.replace('"',''), 'queued load of slot {n} → {destination} (applies next frame)', C, 'quickload notice; developer wording; vanilla "Quickloading..."', 'g:sQuickLoading')
R('hud message', SC, 'format!("write failed: {e}")', 'write failed: {error}', E, 'save failure; operator wording, no vanilla equivalent shown as HUD text')
LL = 'byroredux/src/save_io/live_load.rs'
for needle, why in (('save load ABORTED — cannot reload cell', 'cell reload failure'), ('save load ABORTED — cannot rebuild worldspace', 'worldspace rebuild failure'), ('save load ABORTED — snapshot columns failed typed preflight', 'snapshot preflight failure'), ('save load: resource restore failed', 'resource restore failure'), ('save load: delta apply failed after preflight', 'delta apply failure')):
    R('hud message', LL, needle, needle + ' ...', E, 'load failure pushed through notify_player as a HUD line - ' + why + '; operator diagnostics with internal terms and a debug error chain; no vanilla equivalent shown to the player')
# loading tip / objectives / dialogue passthrough
R('loading', HUD, 'tip.to_owned()', '(loading tip text)', E, 'passthrough of game data (LSCR/loading-tip text) - not an engine string; listed so a per-page test knows the draw site')
rows_sorted = rows
with open('/mnt/data/src/gamebyro-redux/docs/data/game-ui/native-page-strings.tsv', 'w', encoding='utf-8') as f:
    f.write('file_line\tstring\tpage\tdisposition\treason\tgame_string_candidates\tsource\n')
    for fl, s, page, disp, why, cands in rows_sorted:
        f.write('\t'.join(x.replace('\t', ' ').replace('\n', ' ') for x in (fl, s, page, disp, why, cands, 'py/page_strings.py (line located by search); candidates: exact-value index of FO3 exe GMST table + FNV/FO3 ESM GMST + Skyrim/FO4 translate_en')) + '\n')
print(len(rows), 'rows')
