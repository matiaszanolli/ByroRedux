# xWMA Phase 1 corpus results

The four local vanilla archives contained the expected 198,700 entries.
The wrapper reached clean EOF at the declared frame count for 198,073.
All 627 failures were short against `dpds`. The wrapper returned the
decoded PCM before reporting the frame-count error.
[PRD scenario 1](../prd/xwma-audio.md#72-acceptance).

| Archive | Entries | Clean EOF | Short against `dpds` | Largest FFmpeg frame | Largest Rust chunk | Frame cap rejections |
|---|---:|---:|---:|---:|---:|---:|
| `Skyrim - Sounds.bsa` | 269 | 269 | 0 | 30,720 | 2,048 | 0 |
| `Fallout4 - Sounds.ba2`, 6,463 effects and 459 music | 6,922 | 6,840 | 82 | 30,720 | 2,048 | 0 |
| `Skyrim - Voices_en0.bsa` | 75,408 | 75,019 | 389 | 30,720 | 2,048 | 0 |
| `Fallout4 - Voices.ba2` | 116,101 | 115,945 | 156 | 30,720 | 2,048 | 0 |

Frame counts are per channel. Each archive reached the native maximum at
44.1 kHz. The short entries in Skyrim voices were short by 512 (365 files),
768 (15), 896 (5), or 960 (4) frames. Fallout 4 voices were short by
512 (146), 768 (8), or 960 (2) frames. The sound archive list follows below.

The V3 command with FFmpeg `8.1.2-2ubuntu2` yielded equal channels, sample
rate and decoded frame count for all 198,700 entries. The peak absolute
sample difference was one signed 16-bit PCM count in each archive. The
CLI produced the same shorter length on all 627 files, so the differential
check passed while the strict `dpds` coverage check failed.

XWMA-DEC-006 passed on 16 fixed files, four per archive. Fresh processes with
default and forced-zero `av_force_cpu_flags` yielded identical counts and
interleaved `f32` PCM digests. Of 1,000 fixed-seed single-bit mutations
in `fmt `, `dpds` and `data`, 469 completed decode and 531 returned an error
within five seconds and a 256 MiB address-space limit per child. The
playback truncation exercise in scenario 6 belongs to Phase 4.

## Reproducing the checks

Set `BYROREDUX_SKYRIMSE_DATA` and `BYROREDUX_FO4_DATA` to the two games'
Data directories. The CLI and child-process checks use Linux memory files.
The differential check requires `ffmpeg` and `ffprobe` on `PATH`. It uses
a seekable RAM-backed input file for the exact V3 command and probes the
channel count and sample rate separately. The per-file report holds counts,
digests and diagnostics. It defaults to `target/xwma-phase1.tsv`. Set
`XWMA_CORPUS_REPORT` to place it elsewhere under the workspace's `target/`.
[V3 command](xwma-phase0-findings.md#v3--candidate-oracle-and-tolerance).

```sh
cargo test -p byroredux-xwma --test corpus decode_all_to_declared_end -- --ignored --nocapture
cargo test -p byroredux-xwma --test corpus compare_every_entry_with_v3_cli -- --ignored --nocapture
cargo test -p byroredux-xwma --test corpus cpu_flags_are_bit_identical -- --ignored --nocapture
cargo test -p byroredux-xwma --test corpus bit_flips_terminate_within_limits -- --ignored --nocapture
```

## Microsoft decoder adjudication

Use the same hashed xWMA payloads on Windows, including the short voice files
and controls that matched `dpds`. Configure Microsoft's Windows Media Audio
Standard decoder (`CLSID_CWMADecMediaObject`) for the original stream shape
and verify the encoder's extended `WAVEFORMATEX` data required by the
decoder. These xWMA headers have zero `cbSize`. Establish the correct
setup if the native decoder requires extra data. Feed the original
`block_align` packets in order, drain through `MF_E_TRANSFORM_NEED_MORE_INPUT`,
and record native-rate PCM channel count, frame count, digest and the first
sample and frame-count differences from FFmpeg. Compare the leading and
trailing output and check whether any extra frames are decoded audio or
codec padding. [Windows Media Audio Decoder](https://learn.microsoft.com/en-us/windows/win32/medfound/windowsmediaaudiodecoder),
[Configuring Audio Decoding](https://learn.microsoft.com/en-us/windows/win32/medfound/configuringaudiodecoding),
[MFT drain](https://learn.microsoft.com/en-us/windows/win32/medfound/mft-message-command-drain).

Also measure playback of the original xWMA through XAudio2.9, passing the
original `dpds` as `XAUDIO2_BUFFER_WMA`. Capture the source voice PCM at its
native rate before device mixing. Distinguish the playback length from the
raw decoder output, since the playback API receives the declared length as
an input. The Windows measurements are pending. [XAUDIO2_BUFFER_WMA](https://learn.microsoft.com/en-us/windows/win32/api/xaudio2/ns-xaudio2-xaudio2_buffer_wma).

## Fallout 4 sound entries short against `dpds`

Paths refer to members of `Fallout4 - Sounds.ba2`.

**512 frames short (81 entries)**

```text
sound\fx\-18dbfs_1khz_stereo.xwm
sound\fx\amb\elevatorbuttonshort\amb_elevatorbuttonshort_10.xwm
sound\fx\amb\exteriors\glowingsea\amb_ext_glowingsea_pool_steam_02_2d.xwm
sound\fx\amb\exteriors\glowingsea\amb_ext_glowingsea_pool_water_01_2d.xwm
sound\fx\amb\exteriors\harbor\amb_harbor_dock_creak_a_03.xwm
sound\fx\amb\interiors\industrial\amb_int_industrial_rumble_2d_a_01.xwm
sound\fx\amb\interiors\industrialsaugus\amb_int_industrial_saugus_region_creak_a_04.xwm
sound\fx\amb\interiors\industrialsaugus\amb_int_industrial_saugus_region_creak_b_01.xwm
sound\fx\amb\interiors\mq203kelloggsbrain\amb_int_mq203_kelloggs_brain_spot_rumble_a_01.xwm
sound\fx\amb\interiors\mq203kelloggsbrain\amb_int_mq203_kelloggs_brain_spot_wind_a_2d_01.xwm
sound\fx\amb\interiors\shipwreck\amb_int_shipwreck_bed_a_01.xwm
sound\fx\amb\water\interior\amb_water_interior_instituteconcourse_spot_slosh_3d_01.xwm
sound\fx\amb\water\interior\amb_water_interior_instituteconcourse_spot_slosh_3d_02.xwm
sound\fx\amb\water\interior\amb_water_interior_instituteconcourse_spot_slosh_3d_03.xwm
sound\fx\drs\vaultvertical\drs_vault_door_vertical_close_b_01.xwm
sound\fx\drs\vaultvertical\drs_vault_door_vertical_close_b_02.xwm
sound\fx\drs\vaultvertical\drs_vault_door_vertical_open_b_01.xwm
sound\fx\drs\vaultvertical\drs_vault_door_vertical_open_b_02.xwm
sound\fx\drs\vaultvertical\drs_vault_door_vertical_open_b_03.xwm
sound\fx\drsc\race\drsc_race_npc_close_02.xwm
sound\fx\drsc\suitcase\drsc_suitcase_open_01.xwm
sound\fx\fst\npc\barefoot\fst_npc_barefoot_wood_run_04.xwm
sound\fx\itm\flora\itm_flora_razorgrain_harvest_01.xwm
sound\fx\mus\radio\institute\mus_institute_bach_suiteineminorbwv996.xwm
sound\fx\mus\radio\institute\mus_institute_elgar_salutdamor.xwm
sound\fx\mus\radio\institute\mus_institute_rimskykorsakov_scheherezade.xwm
sound\fx\mus\radio\mus_patriotic_presidential.xwm
sound\fx\npc\bloodbug\npc_bloodbug_death_01.xwm
sound\fx\npc\human\eatingdrinking\plate\npc_human_eatingplate_working_06.xwm
sound\fx\npc\human\eatingdrinking\plate\npc_human_eatingplate_working_12.xwm
sound\fx\npc\human\eatingdrinking\plate\npc_human_eatingplate_working_13.xwm
sound\fx\npc\human\eatingdrinking\plate\npc_human_eatingplate_working_18.xwm
sound\fx\npc\human\killpaired\pairedh2h_slipcrosscounter.xwm
sound\fx\npc\human\sit\npc_human_child_front_sit_chair_down_01.xwm
sound\fx\npc\human\typing\npc_human_typing_07.xwm
sound\fx\npc\human\wrench\npc_human_wrench_generic_04.xwm
sound\fx\npc\molerat\injured\npc_molerat_injured_recoil_b_02.xwm
sound\fx\npc\radroach\attack\npc_radroach_attack_jump_02.xwm
sound\fx\npc\radroach\attack\npc_radroach_attack_left_02.xwm
sound\fx\npc\robot\assaultron\attack\npc_robot_assaultron_attack_power_forward_01.xwm
sound\fx\npc\supermutant\idle\cauldron\npc_supermutant_idle_cauldron_taste_01.xwm
sound\fx\npc\supermutantbehemoth\npc_supermutantbehemoth_walk_dirt_hi_04.xwm
sound\fx\npc\supermutantbehemoth\npc_supermutantbehemoth_walk_dirt_lo_03.xwm
sound\fx\npc\supermutantbehemoth\npc_supermutantbehemoth_walk_dirt_lo_04.xwm
sound\fx\phy\body\phy_body_medium_wood_h_03.xwm
sound\fx\qst\mq203kelloggsbrain\qst_mq203_memory_den_memory_exit_01.xwm
sound\fx\qst\ms04recordscratch\qst_ms04recordscratch_needleup_02.xwm
sound\fx\trp\cymbalmonkey\trp_cymbal_monkey_arm_01.xwm
sound\fx\trp\trp_pressure_plate_down_01.xwm
sound\fx\trp\trp_trigger_beep_arm_01.xwm
sound\fx\trp\trp_trigger_beep_fire_01.xwm
sound\fx\ui\lockpicking\ui_lockpicking_cylinderturn_02.xwm
sound\fx\ui\mods\components\screws\ui_mods_components_screws_01.xwm
sound\fx\ui\perk\menu\ui_perk_menu_strong_back_01.xwm
sound\fx\ui\perk\menu\ui_perkmenu_armorer_01.xwm
sound\fx\ui\perk\menu\ui_perkmenu_explorer_01.xwm
sound\fx\ui\perk\menu\ui_perkmenu_leadbelly_01.xwm
sound\fx\ui\pipboy\burstdrivea\ui_pipboy_burstdrivea_02.xwm
sound\fx\ui\pipboy\burstdrivea\ui_pipboy_burstdrivea_17.xwm
sound\fx\ui\pipboy\game\atomcommand\ui_pipboy_game_atomcommand_citybonus_01.xwm
sound\fx\ui\pipboy\game\atomcommand\ui_pipboy_game_atomcommand_citybonus_02.xwm
sound\fx\ui\pipboy\game\atomcommand\ui_pipboy_game_atomcommand_point_01.xwm
sound\fx\ui\pipboy\ping\ui_pipboy_ping_test_pulse_01.xwm
sound\fx\ui\pipboy\ping\ui_pipboy_ping_test_pulse_02.xwm
sound\fx\ui\pipboy\radio\staticbursts\ui_pipboy_radio_staticburst_03.xwm
sound\fx\ui\pipboy\radio\staticbursts\ui_pipboy_radio_staticburst_07.xwm
sound\fx\ui\pipboy\radio\staticbursts\ui_pipboy_radio_staticburst_09.xwm
sound\fx\ui\pipboy\radio\staticbursts\ui_pipboy_radio_staticburst_12.xwm
sound\fx\ui\terminal\harddrive\a\ui_terminal_harddrive_a_05.xwm
sound\fx\ui\terminal\harddrive\a\ui_terminal_harddrive_a_09.xwm
sound\fx\ui\terminal\harddrive\a\ui_terminal_harddrive_a_12.xwm
sound\fx\ui\terminal\harddrive\a\ui_terminal_harddrive_a_14.xwm
sound\fx\ui\terminal\harddrive\b\ui_terminal_harddrive_b_02.xwm
sound\fx\ui\terminal\harddrive\b\ui_terminal_harddrive_b_08.xwm
sound\fx\ui\terminal\harddrive\b\ui_terminal_harddrive_b_10.xwm
sound\fx\ui\terminal\ui_hacking_passbad.xwm
sound\fx\ui\terminal\ui_hacking_passgood.xwm
sound\fx\ui\workshop\powerarmor\ui_workshop_powerarmor_weldmask_a_01.xwm
sound\fx\ui\workshop\powerarmor\ui_workshop_powerarmor_weldmask_b_02.xwm
sound\fx\voc\roboteyebot\voc_eyebot_emergency_broadcast_01.xwm
sound\fx\voc\shaunbaby\cry\voc_shaunbaby_cry_06.xwm
```

**960 frames short (1 entry)**

```text
sound\fx\mus\magnolia\mus_magnolia_traintrain_radio.xwm
```
