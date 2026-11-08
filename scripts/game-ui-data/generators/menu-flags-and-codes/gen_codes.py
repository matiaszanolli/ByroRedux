import re
exec(open('gen_flags2.py').read().split('GECKMM=')[0].split("# FO3 / FNV")[0].split('def scan')[0])
OUT='/mnt/data/src/gamebyro-redux/docs/data/game-ui/'
H=['code','name_geck_cs','name_xedit','name_nvse_or_obse','vanilla','source','note']
def w(fn,rows):
    with open(OUT+fn,'w',encoding='utf-8') as f:
        f.write('\t'.join(H)+'\n')
        for r in rows:
            assert len(r)==len(H),r
            f.write('\t'.join((str(x) if str(x)!='' else '-') for x in r)+'\n')
GT='https://geck.uesp.net/wiki/Template:MenuTypes (local dump geck-uesp-wiki/Template/MenuTypes.wiki)'
CT='https://cs.uesp.net/wiki/Template:MenuTypes (local dump cs-uesp-wiki/Template/MenuTypes.wiki)'
XE='https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsCommon.pas `TwbGameDefCommon.wbMenuModeEnum` (also /mnt/data/tmp/wbDefinitionsCommon.pas)'
NVS='https://github.com/xNVSE/NVSE/blob/0ccd23ad885ddae533c1790a3fc56cd073e38de3/nvse/nvse/GameUI.h `kMenuType_*` + GameUI.cpp `s_menuNameToID`'
OBS='https://github.com/llde/xOBSE/blob/99930e168e73843f412dcd3fdfae8a04f4d3bbad/obse/obse/GameMenus.h `kMenuType_*`'
EXE='<game>.exe entity table (strings `&XMenu;`, `strings -a`)'
# ----- FO3/FNV
nvse_codes={}
nv=open('nvse_GameUI.cpp').read(); body=open('nvse_GameUI.h').read()
body=body[body.index('kMenuType_Min'):body.index('kMenuType_Max =')]
vals={}; cur=None
for l in body.splitlines():
    m=re.match(r'\s*(kMenuType_\w+)\s*(?:=\s*([^,]+))?,',l)
    if not m: continue
    n,v=m.groups()
    if v:
        v=v.strip(); cur=int(v,16) if v.startswith('0x') else vals[v]
    else: cur+=1
    vals[n]=cur
mapn={c:k for k,c in re.findall(r'\{"(\w+)", (kMenuType_\w+)\}',nv)}
strn={v:((mapn.get(n)),n) for n,v in vals.items() if n not in('kMenuType_Min','kMenuType_Max')}
# GECK names (from template)
geck={1001:'Message',1002:'Inventory',1003:'Stats',1004:'HUDMainMenu',1007:'Loading',1008:'Container',1009:'Dialog',1012:'SleepWait',1013:'Pause',1014:'LockPick',1016:'Quantity',1023:'Pipboy Data',1027:'LevelUp',1035:'Pipboy Repair',1036:'RaceSex',1047:'Credits',1048:'CharGen',1051:'TextEdit',1053:'Barter',1054:'Surgery',1055:'Hacking',1056:'VATS',1057:'Computers',1058:'Vendor Repair',1059:'Tutorial',1060:"You're SPECIAL book",1061:'Item Mod Menu (New Vegas)',1074:'Love Tester (New Vegas)',1075:'Companion Wheel (New Vegas)',1076:'The Medical Questionnaire (Not Used) (New Vegas)',1077:'Recipe (New Vegas)',1080:'Slot Machine MiniGame (New Vegas)',1081:'Blackjack Table MiniGame (New Vegas)',1082:'Roulette Table MiniGame (New Vegas)',1083:'Caravan MiniGame (New Vegas)',1084:'Character Creation Traits (not the Medical Questionnaire) (New Vegas)'}
xe={1001:'Message',1002:'Inventory',1003:'Stats',1004:'MainMenu',1007:'Loading',1008:'Container',1009:'Dialog',1012:'Sleep/Wait',1013:'Pause',1014:'Lockpick',1016:'Quantity',1023:'Pipboy: Data',1026:"Book Menu (LStewieAI's Book Menu Restored)",1027:'Level Up',1035:'Pipboy: Repair',1036:'Race Menu/Barber Menu/Plastic Surgery Menu',1047:'Credits',1048:'Character Creation',1051:'Text Edit',1053:'Barter',1054:'Surgery',1055:'Hacking',1056:'VATS',1057:'Computers',1058:'Vendor Repair',1059:'Tutorial',1060:"You're SPECIAL Book",1061:'Item Mod Menu',1069:"Tweaks Menu (LStewieAI' Tweaks)",1074:'Love Tester',1075:'Companion Wheel',1076:'The Medical Questionnaire (Unused)',1077:'Recipe',1080:'Minigame: Slot Machine',1081:'Minigame: Blackjack',1082:'Minigame: Roulette',1083:'Minigame: Caravan',1084:'Character Creation Traits'}
xe_fo3_only=set(range(1053,1061))
rows=[]
allc=sorted(set(geck)|set(xe)|set(strn))
fo3_ent={1001:1,1002:1,1003:1,1004:1,1007:1,1008:1,1009:1,1012:1,1013:1,1014:1,1016:1,1023:1,1026:1,1027:1,1035:1,1036:1,1047:1,1048:1,1051:1,1053:1,1054:1,1055:1,1056:1,1057:1,1058:1,1059:1,1060:1}
for c in allc:
    nvn=(f'{strn[c][1]} ("{strn[c][0]}")' if strn[c][0] else f'{strn[c][1]} (enum only; no s_menuNameToID string)') if c in strn else '-'
    gn=geck.get(c,'-'); xn=xe.get(c,'-')
    vanilla='yes' if c in strn else 'mod-added'
    srcs=[]
    if c in geck: srcs.append(GT)
    if c in xe: srcs.append(XE)
    if c in strn: srcs.append(NVS)
    note=[]
    if c==1004:
        note.append('SETTLED (GUI-REG-006): vanilla name is HUDMainMenu. xNVSE names it kMenuType_HUDMain with s_menuNameToID "HUDMainMenu" and class `HUDMainMenu : public Menu // 1004` (GameUI.h); the game executables hold the entity `&HUDMainMenu;`; menus\\main\\hud_main_menu.xml is `<menu name="HUDMainMenu">` with `<class> &HUDMainMenu; </class>` and `<stackingtype> &does_not_stack;` (Fallout - Misc.bsa, FO3 and FNV). xEdit "MainMenu" is a mislabel: the title-screen menu is a separate document, menus\\options\\main_menu.xml (`<menu name="MainMenu">`, `<class> &MainMenu; </class>`, no numeric code in the xNVSE enum). GECK note: at the title screen MenuMode 1004/1007/1013 return true.')
    if c==1013: note.append('GECK "Pause" vs xNVSE kMenuType_Start/"StartMenu": same code; game has both start_menu.xml (`StartMenu`, exe entity `&StartMenu;`) and pause_menu.xml (`PauseMenu`, no exe entity) - xNVSE assigns 1013 to StartMenu; the PauseMenu document has no numeric code in the xNVSE enum.')
    if c in (1061,1074,1075,1076,1077,1080,1081,1082,1083,1084): note.append('FNV only: not in the Fallout3.exe entity table.')
    if c in range(1053,1061): note.append('xEdit wraps these in IsFO3(..., \'\') so it names them only when the plugin game is Fallout 3; an FNV plugin gets no label from xEdit although the code exists (xNVSE names it).')
    if c==1026: note.append('xEdit credits the label to LStewieAI\'s Book Menu Restored (mod); the code/name kMenuType_Book "BookMenu" and bookmenu document are vanilla (xNVSE, exe `&BookMenu;`, book_menu.xml). Not named by GECK.')
    if c==1069: note.append('Mod-added: xEdit credits "Tweaks Menu (LStewieAI\' Tweaks" (mod). No xNVSE vanilla constant. Not vanilla.')
    if c==1076: note.append('GECK/xEdit label it the unused Medical Questionnaire; xNVSE kMenuType_TraitSelect "TraitSelectMenu"; exe `&TraitSelectMenu;`.')
    if c==1084: note.append('xNVSE kMenuType_Trait "TraitMenu"; exe `&TraitMenu;`.')
    if c==1023: note.append('xNVSE kMenuType_Map "MapMenu" (exe `&MapMenu;`): the Pipboy DATA tab.')
    if c==1035: note.append('xNVSE kMenuType_Repair "RepairMenu".')
    if c==1058: note.append('xNVSE kMenuType_RepairServices "RepairServicesMenu".')
    if c==1060: note.append('xNVSE kMenuType_SpecialBook "SpecialBookMenu" (exe `&SPECIALBookMenu;`).')
    if c not in strn and c in geck: note.append('Named by GECK only.')
    if c in strn and c not in geck and c not in xe: note.append('Named by xNVSE only (GECK template elides it with "...", xEdit has no entry).')
    rows.append((c,gn,xn,nvn,vanilla,'; '.join(srcs),' '.join(note)))
# menu types
for c,n in ((0,'(GameMode: no menu)'),(1,'"main four" (the character interface: stats, magic, inventory, quest log)'),(2,'any other menu (message boxes, containers, etc.)'),(3,'console (opened with ~)')):
    rows.insert(c,(c,n if c else '(GameMode)',{0:'Any',1:'Type: Character Interface',2:'Type: Other',3:'Type: Console'}[c],'-','yes',GT+'; '+XE,'Menu TYPE argument of MenuMode, not a specific menu.' if c else 'xEdit enum value 0 = Any (MenuMode with no argument).'))
rows.insert(4,(4,'-','Title Screen (JIP LN)','-','mod-added',XE,'xEdit labels FNV value 4 as JIP LN (mod); not vanilla.'))
for n in ('AudioMenu','VideoMenu','GameplayMenu'):
    rows.append(('unknown','-','-',f'- (exe entity `&{n};`)','yes','strings -a FalloutNV.exe / Fallout3.exe (`&%s;` present in both)'%n,'Options sub-menu present as a menu entity but no source (GECK elides, xEdit/xNVSE omit) gives its FO3/FNV numeric code. Oblivion\'s entity values are 1017/1018/1020 (CS Entities) but the FO3/FNV values are not sourced.'))
w('menu-codes-fnv-fo3.tsv',rows)
print('fnv',len(rows))
# ----- Oblivion
cs_names={}
for l in open('/mnt/data/src/reference/cs-uesp-wiki/Template/MenuTypes.wiki'):
    m=re.match(r'\s*(\d{4}) = (.*)',l)
    if m: cs_names[int(m.group(1))]=m.group(2).strip()
xe_t4={1001:'Message',1002:'Inventory',1003:'Stats',1004:'HUD: Main',1005:'HUD: Info',1006:'HUD: Reticle',1007:'Loading',1008:'Container/Barter',1009:'Dialog',1010:'HUD: Subtitle',1011:'Generic',1012:'Sleep/Wait',1013:'Pause',1014:'Lockpick',1015:'Options',1016:'Quantity',1017:'Audio',1018:'Video',1019:'Video Display',1020:'Gameplay',1021:'Controls',1022:'Magic',1023:'Map',1024:'Magic Popup',1025:'Negotiate',1026:'Book',1027:'Level Up',1028:'Training',1029:'Birthsign',1030:'Class',1031:'Attributes',1032:'Skills',1033:'Specialization',1034:'Persuasion',1035:'Repair/Ingredient Selection',1036:'Race Menu',1037:'Spell Purchase',1038:'Load',1039:'Save',1040:'Alchemy',1041:'Spellmaking',1042:'Enchantment',1043:'EffectSetting',1044:'Main',1045:'Breath',1046:'QuickKeys',1047:'Credits',1048:'Sigil Stone',1049:'Recharge',1051:'Text Edit'}
ob=open('obse_GameMenus.h').read()
ob=ob[ob.index('kMenuType_None'):ob.index('kMenuType_Max')]
ov={}; cur=None
for l in ob.splitlines():
    m=re.match(r'\s*(kMenuType_\w+)\s*(?:=\s*(\w+))?\s*,?',l)
    if not m: continue
    n,v=m.groups()
    cur=int(v,0) if v else (cur+1 if cur is not None else 0)
    ov[cur]=n
ent=open('/mnt/data/src/reference/cs-uesp-wiki/(main)/Oblivion XML Reference/Entities.wiki').read()
EN={int(v,16):n for n,v,m in re.findall(r'\|&amp;(\w+);\n\|(0x[0-9A-Fa-f]+)\n\|([^\n]*)',ent) if 'Menu ID' in m or n in('HUDReticle',)}
rows=[]
ENT='https://cs.uesp.net/wiki/Oblivion_XML_Reference/Entities (local dump cs-uesp-wiki/(main)/Oblivion XML Reference/Entities.wiki) + Oblivion.exe entity strings'
for c in sorted(set(cs_names)|set(xe_t4)|set(ov)|set(EN)):
    if c<1000: continue
    srcs=[]
    if c in cs_names: srcs.append(CT)
    if c in xe_t4: srcs.append(XE)
    if c in ov: srcs.append(OBS)
    if c in EN: srcs.append(ENT)
    van='yes' if (c in EN) else 'unknown'
    note=[]
    if c in EN: note.append(f'XML entity &{EN[c]}; (Menu ID).')
    if c in (1031,1033): note.append('Named Attributes/Specialization by CS template and xEdit only; OBSE leaves it unk%x and no `&%sMenu;` entity exists in Oblivion.exe or the CS entity list; no menu document in Oblivion - Misc.bsa. Vanilla status not established.'%(c,'Attributes' if c==1031 else 'Specialization'))
    if c==1050: note.append('No source names it (OBSE kMenuType_unk41A).')
    if c==1004: note.append('Oblivion xEdit "HUD: Main" matches CS "HUDMain" and the entity &HUDMainMenu;; the same code is HUDMainMenu on FO3/FNV (xEdit mislabels it MainMenu there).')
    if c==1008: note.append('Oblivion barter and container share 1008 (CS "Container, Barter"; xEdit "Container/Barter"); FO3/FNV barter is 1053.')
    if c==1044: note.append('Title screen MainMenu (&MainMenu;).')
    rows.append((c,cs_names.get(c,'-'),xe_t4.get(c,'-'),ov.get(c,'-').replace('kMenuType_','kMenuType_'),van,'; '.join(srcs),' '.join(note)))
rows.insert(0,(0,'(GameMode)','Any','kMenuType_None','yes',OBS+'; '+XE,'No menu / GameMode; MenuMode with no argument.'))
rows.insert(1,(1,'"main four" (stats, magic, inventory, quest log)','Type: Character Interface','kMenuType_BigFour','yes',CT+'; '+OBS,'OBSE comment: "F1 - F4 menus".'))
rows.insert(2,(2,'any other menu','Type: Other','-','yes',CT+'; '+XE,''))
rows.insert(3,(3,'console','Type: Console','-','yes',CT+'; '+XE,''))
w('menu-codes-oblivion.tsv',rows)
print('obl',len(rows))
