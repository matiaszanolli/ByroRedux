# #1326 — NiPSysGravityModifier World Aligned gate

_Filed + fixed 06be205b from the #1306 follow-up trace._

nif.xml: World Aligned vercond=!#NI_BS_LTE_16# (bsver>16). Was gated version>=V20_0_0_4 → Oblivion 1-byte over-read. Fixed to bsver>16.
