# Persistent full models, RND-40

This document retains the earlier RND-40 residency text and the incoming
RND-41 decision. The RND-41 visibility decision governs the merged renderer.
Earlier counts and whole-world observations describe their original checkpoints.

## Persistent full models, RND-40 and RND-41

The baseline survey ran on 2026-10-13 against Skyrim.esm, Update.esm,
Dawnguard.esm, HearthFires.esm and Dragonborn.esm, before changing selection.
The Whiterun gate XTEL landing is [19243.535, -7427.343, -3595.4053],
in Tamriel grid cell (4, -2). Near means within 20 cells on each grid axis.
The loaded grid uses radius 2. The table records full-model selection at
that landing before the fix. Generated object LOD excludes these persistent
references, so an enabled modeled reference outside that selection had no
rendering owner. The two disabled references keep their authored enable state.
Dragon perch furniture supplies editor markers rather than game-visible
geometry. Its NIF shapes are named EditorMarker, which the local OpenMW
nifloader.cpp also excludes. Full LOD does not make those markers visible.

The rule follows local UESP REFR.wikitext, Wrye Bash
[dev]-Record-Header-Flags.md, TES5Edit/Core/wbDefinitionsTES5.pas and
CommonLibSSE TESObjectREFR.h and TESObjectSTAT.h. REFR bit 16 selects Full LOD.
STAT header bit 2 selects Never Fades. A similarly numbered bit on another
base type does not select Never Fades.

Full models remain owned by their worldspace persistent cell during grid
movement. Ordinary persistent geometry stays in its physical grid cell.

STAT and MSTT header bit 2 selects Never Fades in xEdit. A similarly numbered
bit on an unrelated base type does not select Never Fades. The earlier survey
below counted only STAT Never Fades. The continuation corrects MSTT selection
and keeps the MISC flag exclusion in the synthetic ownership test.

The 2026-10-13 visibility decision supersedes whole-world residency.
Inside loaded grid cells, both kinds use physical presentation. Beyond those
cells, only visible demand is owned by the worldspace persistent cell.
Collision uses the physical selection for both kinds. Static full models
share local geometry through MultiMesh groups, including groups below the
ordinary grid instancing threshold. Materials and textures share the existing
path caches. Clearing the scene releases model RIDs, prunes unused Godot
materials and textures, and clears prepared CPU texture entries. Door
transitions and exterior preview worldspace changes use that cleanup.
Logical CELL ownership and game state stay in the record database.
No dependency, saved field or new per-reference node path is added.
Small worlds retain their existing whole-city presentation.

## Vanilla counts

Full and Never overlap. Union counts each reference once.

| Worldspace | FormID | Full | Never | Both | Union |
| --- | --- | ---: | ---: | ---: | ---: |
| Tamriel | 0:00003C | 951 | 0 | 0 | 951 |
| WindhelmWorld | 0:01691D | 1 | 0 | 0 | 1 |
| RiftenWorld | 0:016BB4 | 0 | 0 | 0 | 0 |
| MarkarthWorld | 0:016D71 | 0 | 0 | 0 | 0 |
| WhiterunWorld | 0:01A26F | 0 | 0 | 0 | 0 |
| CWTestHold | 0:01B44A | 0 | 0 | 0 | 0 |
| LabyrinthianWorld | 0:01CDD3 | 0 | 0 | 0 | 0 |
| LabyrinthianWorld03 | 0:01CDD9 | 0 | 0 | 0 | 0 |
| MossMotherCavernWorld | 0:01E49D | 0 | 0 | 0 | 0 |
| Blackreach | 0:01EE62 | 0 | 0 | 0 | 0 |
| BloatedMansGrottoWorld | 0:01FAE2 | 0 | 0 | 0 | 0 |
| FallowstoneCaveWorldEnd | 0:0204C7 | 0 | 0 | 0 | 0 |
| SouthfringeWorld | 0:020BFE | 0 | 0 | 0 | 0 |
| BlindCliffCaveWorld | 0:020DCB | 0 | 0 | 0 | 0 |
| LabyrinthianWorld04 | 0:021EDB | 0 | 0 | 0 | 0 |
| FallowstoneCaveWorldStart | 0:0243DE | 0 | 0 | 0 | 0 |
| SkuldafnWorld | 0:0278DD | 40 | 0 | 0 | 40 |
| DeepwoodRedoubtWorld | 0:029AB7 | 0 | 0 | 0 | 0 |
| ShadowgreenCavernWorld | 0:02A9D8 | 0 | 0 | 0 | 0 |
| RedEagleRedoubtWorld | 0:02B101 | 0 | 0 | 0 | 0 |
| DarkwaterWorld | 0:02C965 | 0 | 0 | 0 | 0 |
| Sovngarde | 0:02EE41 | 60 | 0 | 0 | 60 |
| BluePalaceWingWorld | 0:034240 | 0 | 0 | 0 | 0 |
| KarthspireRedoubtWorld | 0:035699 | 0 | 0 | 0 | 0 |
| SolitudeWorld | 0:037EDF | 0 | 0 | 0 | 0 |
| EldergleamSanctuaryWorld | 0:03A9D6 | 0 | 0 | 0 | 0 |
| CWSiegeTestWorld | 0:0419E1 | 0 | 0 | 0 | 0 |
| WhiterunDragonsreachWorld | 0:046033 | 0 | 0 | 0 | 0 |
| EastEmpireWarehouse | 0:04F838 | 14 | 0 | 0 | 14 |
| BrinewaterGrottoWorld | 0:050015 | 0 | 0 | 0 | 0 |
| AlftandWorld | 0:069857 | 0 | 0 | 0 | 0 |
| JaphetsFollyWorld | 0:094B35 | 48 | 0 | 0 | 48 |
| KatariahWorld | 0:0C350D | 0 | 0 | 0 | 0 |
| FrostmereCryptWorld | 0:0C97EB | 0 | 0 | 0 | 0 |
| WindhelmPitWorldspace | 0:0D45F0 | 0 | 0 | 0 | 0 |
| LabyrinthianMazeWorld | 0:104217 | 1 | 0 | 0 | 1 |
| DLC01FalmerValley | 2:000BB5 | 80 | 0 | 0 | 80 |
| DLC01SoulCairn | 2:001408 | 44 | 0 | 0 | 44 |
| DLC1HunterHQWorld | 2:001DB8 | 10 | 0 | 0 | 10 |
| DLC1ForebearsHoldout | 2:002F64 | 0 | 0 | 0 | 0 |
| TestMeganWorld | 2:0042C2 | 0 | 0 | 0 | 0 |
| DLC1AncestorsGladeWorld | 2:0048C7 | 0 | 0 | 0 | 0 |
| DLC1DarkfallPassageWorld | 2:004BEA | 0 | 0 | 0 | 0 |
| DLC01Boneyard | 2:00528D | 0 | 0 | 0 | 0 |
| DLC1VampireCastleCourtyard | 2:007202 | 0 | 0 | 0 | 0 |
| DLC2SolstheimWorld | 4:000800 | 16 | 1 | 1 | 16 |
| DLC2ApocryphaWorld | 4:01C0B2 | 30 | 0 | 0 | 30 |

## Tamriel references near the gate

There are 363 flagged references here, including 361 enabled references with
models. Of those 361, 110 are marker-only dragon perch furniture.
The remaining 251 supply visible geometry. Before the fix only
WRLODWindowGlow01 was selected for a full model. The installed rendering
check now submits all 251 and all 785 drawable full models in Tamriel.
Returning through the gate retains zero Tamriel full-model references.

The remaining 251 supply visible geometry. Before RND-40 only
WRLODWindowGlow01 was selected for a full model. The historical RND-40 check
submitted all 251 and all 785 drawable full models in Tamriel. RND-41 selects
visible demand instead. Returning through the gate must retain zero Tamriel
full-model references.

| Reference | Base editor ID | Grid | Enabled | Model | Full model before |
| --- | --- | --- | --- | --- | --- |
| 0:021D19 | SkyrimCloudDistant06 | 3,-14 | true | true | false |
| 0:021D1D | SkyrimCloudDistant06 | 7,-15 | true | true | false |
| 0:021D29 | SkyrimCloudDistant06 | 8,-20 | true | true | false |
| 0:021D43 | SkyrimCloudDistant05 | 10,-18 | true | true | false |
| 0:021D44 | SkyrimCloudDistant03 | 7,-14 | true | true | false |
| 0:021D75 | SkyrimCloudDistant03 | 9,-12 | true | true | false |
| 0:021F4F | SkyrimCloudDistant04 | 9,-15 | true | true | false |
| 0:02630F | SkyrimCloudDistant01 | 8,-10 | true | true | false |
| 0:026312 | SkyrimCloudDistant02 | 5,-16 | true | true | false |
| 0:034B08 | DragonPerchTower | -11,8 | true | true | false |
| 0:040890 | DragonPerchTower | -10,8 | true | true | false |
| 0:040891 | DragonPerchTower | -11,8 | true | true | false |
| 0:040892 | DragonPerchTower | -10,8 | true | true | false |
| 0:0558C2 | DragonPerchWordwall | -4,-12 | true | true | false |
| 0:05EDF5 | DragonPerchWordwall | -2,11 | true | true | false |
| 0:064853 | DragonPerchTower | -11,9 | true | true | false |
| 0:064857 | DragonPerchRockL02 | -11,9 | true | true | false |
| 0:064858 | DragonPerchTower | -10,9 | true | true | false |
| 0:064859 | DragonPerchTower | -10,9 | true | true | false |
| 0:06485B | DragonPerchTower | -5,8 | true | true | false |
| 0:06485C | DragonPerchTower | -5,7 | true | true | false |
| 0:06485D | DragonPerchWordwall | -6,8 | true | true | false |
| 0:06B1EE | DragonPerchRockL02 | 5,-11 | true | true | false |
| 0:06B1EF | DragonPerchRockL02 | 5,-11 | true | true | false |
| 0:06B1F0 | DragonPerchRockL02 | 5,-11 | true | true | false |
| 0:06B1F1 | DragonPerchRockL02 | 6,-12 | true | true | false |
| 0:06B1F2 | DragonPerchRockL02 | 4,-12 | true | true | false |
| 0:06B20B | DragonPerchRockL02 | 5,-12 | true | true | false |
| 0:06B20C | DragonPerchRockL02 | 5,-12 | true | true | false |
| 0:06B3B2 | DragonPerchTower | 5,-11 | true | true | false |
| 0:06B3B4 | DragonPerchRockL02 | 4,-11 | true | true | false |
| 0:06B3B5 | DragonPerchTower | 5,-12 | true | true | false |
| 0:06B3B6 | DragonPerchRockL02 | 5,-12 | true | true | false |
| 0:06B3B7 | DragonPerchTower | 4,-11 | true | true | false |
| 0:06B3BC | DragonPerchRockL02 | 4,-12 | true | true | false |
| 0:06B3BF | DragonPerchTower | 5,-12 | true | true | false |
| 0:06B3C0 | DragonPerchRockL02 | 5,-11 | true | true | false |
| 0:06CF54 | DragonTowerHole01 | 3,-21 | true | true | false |
| 0:073E65 | DragonPerchRockL02 | 24,8 | true | true | false |
| 0:073E67 | DragonPerchRockL02 | 24,9 | true | true | false |
| 0:073FC9 | DragonPerchTower | 24,9 | true | true | false |
| 0:073FCA | DragonPerchRockL02 | 24,9 | true | true | false |
| 0:079B44 | DragonPerchTower | -5,11 | true | true | false |
| 0:079B45 | DragonPerchTower | -5,11 | true | true | false |
| 0:079B46 | DragonPerchTower | -4,10 | true | true | false |
| 0:079B47 | DragonPerchTower | -6,10 | true | true | false |
| 0:079B48 | DragonPerchWordwall | -5,10 | true | true | false |
| 0:079B49 | DragonPerchWordwall | -6,11 | true | true | false |
| 0:079B4A | DragonPerchTower | -6,11 | true | true | false |
| 0:079B4B | DragonPerchTower | -6,11 | true | true | false |
| 0:079B4C | DragonPerchTower | -6,11 | true | true | false |
| 0:079B4D | DragonPerchTower | -6,11 | true | true | false |
| 0:079B4E | DragonPerchTower | -5,12 | true | true | false |
| 0:079B4F | DragonPerchWordwall | -5,12 | true | true | false |
| 0:079B50 | DragonPerchWordwall | -6,11 | true | true | false |
| 0:079B51 | DragonPerchTower | -5,11 | true | true | false |
| 0:079B52 | DragonPerchTower | -5,11 | true | true | false |
| 0:079B53 | DragonPerchTower | -5,11 | true | true | false |
| 0:079B54 | DragonPerchTower | -4,11 | true | true | false |
| 0:079B55 | DragonPerchTower | -5,10 | true | true | false |
| 0:079B56 | DragonPerchTower | -5,10 | true | true | false |
| 0:079B57 | DragonPerchWordwall | -4,9 | true | true | false |
| 0:079B58 | DragonPerchWordwall | -4,9 | true | true | false |
| 0:079B59 | DragonPerchWordwall | -4,9 | true | true | false |
| 0:079B5A | DragonPerchTower | -4,9 | true | true | false |
| 0:079B5B | DragonPerchTower | -4,9 | true | true | false |
| 0:093698 | DragonPerchTower | 0,-4 | true | true | false |
| 0:0936AA | DragonPerchTower | 0,-4 | true | true | false |
| 0:09C21C | DragonPerchRockL02 | 13,-12 | true | true | false |
| 0:09C21F | DragonPerchRockL02 | 14,-12 | true | true | false |
| 0:09C8D4 | WRLODWindowGlow01 | 5,-1 | true | true | true |
| 0:09E077 | DragonPerchWordwall | 13,-13 | true | true | false |
| 0:09E080 | DragonPerchRockL02 | 13,-13 | true | true | false |
| 0:09E083 | DragonPerchWordwall | 13,-13 | true | true | false |
| 0:09E086 | DragonPerchRockL02 | 13,-13 | true | true | false |
| 0:09E089 | DragonPerchRockL02 | 13,-12 | true | true | false |
| 0:09E08C | DragonPerchRockL02 | 13,-12 | true | true | false |
| 0:09E08F | DragonPerchRockL02 | 13,-12 | true | true | false |
| 0:09E092 | DragonPerchTower | 13,-12 | true | true | false |
| 0:09FBED | DragonPerchWordwall | -1,-11 | true | true | false |
| 0:0A1952 | DragonPerchWordwall | 16,4 | true | true | false |
| 0:0A927F | FXMistLowSwirls01 | -10,-5 | true | true | false |
| 0:0AA7DE | DragonPerchTower | 4,-20 | true | true | false |
| 0:0B41D8 | DragonPerchTower | 24,-6 | true | true | false |
| 0:0B41D9 | DragonPerchTower | 24,-6 | true | true | false |
| 0:0B9686 | DragonPerchTower | 24,-6 | true | true | false |
| 0:0C9876 | DragonPerchRockL02 | 22,17 | true | true | false |
| 0:0C9877 | DragonPerchRockL02 | 23,17 | true | true | false |
| 0:0C9878 | DragonPerchRockL02 | 23,16 | true | true | false |
| 0:0C9879 | DragonPerchTower | 24,16 | true | true | false |
| 0:0CEFC6 | DragonPerchTower | 3,-19 | true | true | false |
| 0:0CEFC8 | DragonPerchTower | 4,-20 | true | true | false |
| 0:0D30DE | DragonPerchRockL02 | 23,15 | true | true | false |
| 0:0D56A6 | DragonPerchTower | 4,-20 | true | true | false |
| 0:0D67B9 | DragonPerchTower | 3,-20 | true | true | false |
| 0:0D67BA | DragonPerchTower | 3,-21 | true | true | false |
| 0:0D67BB | DragonPerchTower | 3,-20 | true | true | false |
| 0:0D7D85 | DragonPerchTower | 3,-21 | true | true | false |
| 0:0D9077 | DragonPerchTower | 0,-4 | false | true | false |
| 0:0DB88D | DragonPerchRockL02 | 0,-4 | true | true | false |
| 0:0DB88E | DragonPerchRockL02 | 0,-4 | true | true | false |
| 0:0E1504 | DragonPerchRockL02 | 4,-21 | true | true | false |
| 0:0E1506 | DragonPerchTower | 3,-20 | true | true | false |
| 0:0E1508 | DragonPerchRockL02 | 4,-19 | true | true | false |
| 0:0E1521 | DragonPerchRockL02 | 4,-20 | true | true | false |
| 0:0E3EB3 | DragonPerchRockL02 | -2,12 | true | true | false |
| 0:0E4057 | DragonPerchRockL02 | -2,11 | true | true | false |
| 0:0E405B | DragonPerchRockL02 | -2,11 | true | true | false |
| 0:0F0837 | DragonPerchRockL02 | 5,-11 | true | true | false |
| 0:0F4576 | MagicLightReanimate1 | -10,-21 | false | false | false |
| 0:0F64D1 | FXSwirlsLgVol | -10,-5 | true | true | false |
| 0:0FDACB | DragonPerchRockL02 | -13,-8 | true | true | false |
| 0:0FDACD | DragonPerchRockL02 | -13,-8 | true | true | false |
| 0:0FDACF | DragonPerchRockL02 | -12,-8 | true | true | false |
| 0:0FDAD1 | DragonPerchRockL02 | -13,-8 | true | true | false |
| 0:0FDAD3 | DragonPerchRockL02 | -13,-8 | true | true | false |
| 0:0FDAEF | DragonPerchRockL02 | -14,-18 | true | true | false |
| 0:0FDAF5 | DragonPerchRockL02 | -13,-18 | true | true | false |
| 0:0FDAFC | DragonPerchRockL02 | -9,1 | true | true | false |
| 0:0FDAFE | DragonPerchRockL02 | -9,2 | true | true | false |
| 0:0FDB17 | DragonPerchRockL02 | 10,3 | true | true | false |
| 0:0FDB19 | DragonPerchRockL02 | 10,3 | true | true | false |
| 0:0FDB1B | DragonPerchRockL02 | 11,4 | true | true | false |
| 0:10D00E | SkyrimCloudDistant04_O_25 | 19,1 | true | true | false |
| 0:10D012 | SkyrimCloudShape06_50 | 19,3 | true | true | false |
| 0:10D016 | INV_SkyrimCloudShape01_O | 17,0 | true | true | false |
| 0:10D017 | INV_SkyrimCloudDistant01 | 16,-8 | true | true | false |
| 0:10D019 | SkyrimCloudDistant01 | 15,-5 | true | true | false |
| 0:10D01A | SkyrimCloudShape01_50 | 16,-6 | true | true | false |
| 0:10D27E | INV_SkyrimCloudShape04_O | 16,2 | true | true | false |
| 0:10D27F | INV_SkyrimCloudDistant05_O | 18,0 | true | true | false |
| 0:10D280 | SkyrimCloudDistant02 | 12,-6 | true | true | false |
| 0:10D281 | SkyrimCloudDistant02_O | 13,-5 | true | true | false |
| 0:10D282 | SkyrimCloudDistant02 | 15,-4 | true | true | false |
| 0:10D283 | INV_SkyrimCloudShape03 | 11,-5 | true | true | false |
| 0:10D284 | SkyrimCloudDistant04 | 10,-4 | true | true | false |
| 0:10D285 | SkyrimCloudShape05_50 | 10,-5 | true | true | false |
| 0:10D286 | SkyrimCloudShape05_50 | 10,-5 | true | true | false |
| 0:10D287 | SkyrimCloudDistant03 | 10,-8 | true | true | false |
| 0:10D288 | SkyrimCloudShape01 | 10,-7 | true | true | false |
| 0:10D289 | SkyrimCloudShape01 | 3,-8 | true | true | false |
| 0:10D28A | SkyrimCloudShape02 | 3,-8 | true | true | false |
| 0:10D28B | SkyrimCloudDistant06 | 1,-8 | true | true | false |
| 0:10D28C | SkyrimCloudShape01_50 | 1,-7 | true | true | false |
| 0:10D28D | INV_SkyrimCloudDistant06 | -2,-9 | true | true | false |
| 0:10D28E | SkyrimCloudDistant04 | -2,-10 | true | true | false |
| 0:10D28F | INV_SkyrimCloudShape01 | -2,-11 | true | true | false |
| 0:10D290 | INV_SkyrimCloudShape02_50 | -2,-10 | true | true | false |
| 0:10D292 | INV_SkyrimCloudShape02 | -4,-8 | true | true | false |
| 0:10D296 | INV_SkyrimCloudShape02 | -3,-11 | true | true | false |
| 0:10D5D2 | INV_SkyrimCloudShape05_O_50 | 15,1 | true | true | false |
| 0:10D5D5 | SkyrimCloudShape01 | -14,-11 | true | true | false |
| 0:10D5D6 | INV_SkyrimCloudShape04 | -14,-10 | true | true | false |
| 0:10D5D7 | INV_SkyrimCloudShape05_O | -14,-10 | true | true | false |
| 0:10D5D8 | SkyrimCloudShape03 | -14,-10 | true | true | false |
| 0:10D5DA | INV_SkyrimCloudShape05_O | -16,-7 | true | true | false |
| 0:10D5EC | INV_SkyrimCloudDistant02 | -16,14 | true | true | false |
| 0:10D5EE | INV_SkyrimCloudShape02_O | -16,13 | true | true | false |
| 0:10D5EF | INV_SkyrimCloudShape04 | -16,12 | true | true | false |
| 0:10D5F1 | SkyrimCloudDistant01 | -12,8 | true | true | false |
| 0:10D5F2 | INV_SkyrimCloudDistant01 | -14,9 | true | true | false |
| 0:10D5F3 | SkyrimCloudShape02 | -11,10 | true | true | false |
| 0:10D5F4 | SkyrimCloudShape02 | -13,10 | true | true | false |
| 0:10D5F5 | SkyrimCloudShape03 | -14,8 | true | true | false |
| 0:10D5F6 | INV_SkyrimCloudShape03 | -14,7 | true | true | false |
| 0:10D5F7 | INV_SkyrimCloudDistant03 | -13,7 | true | true | false |
| 0:10D5F8 | INV_SkyrimCloudShape02_50 | -13,7 | true | true | false |
| 0:10D5F9 | SkyrimCloudShape02_50 | -11,7 | true | true | false |
| 0:10D5FA | INV_SkyrimCloudDistant03 | -11,6 | true | true | false |
| 0:10D5FB | INV_SkyrimCloudDistant01 | -11,8 | true | true | false |
| 0:10D5FC | INV_SkyrimCloudDistant01_50 | -10,8 | true | true | false |
| 0:10D5FD | INV_SkyrimCloudDistant03_50 | -10,6 | true | true | false |
| 0:10D5FE | INV_SkyrimCloudDistant03_50 | -10,6 | true | true | false |
| 0:10D5FF | INV_SkyrimCloudDistant03 | -5,7 | true | true | false |
| 0:10D600 | SkyrimCloudDistant02_O | -6,8 | true | true | false |
| 0:10D601 | SkyrimCloudDistant03 | -6,9 | true | true | false |
| 0:10D602 | SkyrimCloudDistant06_O | -7,8 | true | true | false |
| 0:10D603 | SkyrimCloudShape02_50 | -7,9 | true | true | false |
| 0:10D604 | INV_SkyrimCloudShape04 | -5,7 | true | true | false |
| 0:10D605 | INV_SkyrimCloudShape03_O | -6,6 | true | true | false |
| 0:10D606 | INV_SkyrimCloudShape03_O | -5,6 | true | true | false |
| 0:10D607 | INV_SkyrimCloudShape03_O_50 | -4,6 | true | true | false |
| 0:10D608 | INV_SkyrimCloudShape05_50 | -4,6 | true | true | false |
| 0:10D609 | SkyrimCloudDistant02_O | -5,7 | true | true | false |
| 0:10D60A | SkyrimCloudDistant05_O_50 | -4,7 | true | true | false |
| 0:10D60B | INV_SkyrimCloudDistant02_50 | -1,7 | true | true | false |
| 0:10D60C | INV_SkyrimCloudShape06_50 | -1,6 | true | true | false |
| 0:10D60D | INV_SkyrimCloudShape06_50 | -1,7 | true | true | false |
| 0:10D60E | INV_SkyrimCloudDistant05 | 0,7 | true | true | false |
| 0:10D60F | INV_SkyrimCloudShape01_50 | 2,6 | true | true | false |
| 0:10D610 | SkyrimCloudShape01_50 | 1,6 | true | true | false |
| 0:10D611 | SkyrimCloudShape02 | 2,6 | true | true | false |
| 0:10D612 | INV_SkyrimCloudDistant02_50 | 12,2 | true | true | false |
| 0:10D613 | INV_SkyrimCloudShape04_50 | 11,3 | true | true | false |
| 0:10D614 | INV_SkyrimCloudDistant03 | 12,2 | true | true | false |
| 0:10D615 | SkyrimCloudDistant03 | 12,3 | true | true | false |
| 0:10D616 | SkyrimCloudShape02_O | 12,3 | true | true | false |
| 0:10D617 | SkyrimCloudShape02_O | 13,4 | true | true | false |
| 0:10D618 | INV_SkyrimCloudShape04_50 | 13,3 | true | true | false |
| 0:10D619 | INV_SkyrimCloudDistant01_O | 17,2 | true | true | false |
| 0:10D61A | SkyrimCloudShape03 | 15,3 | true | true | false |
| 0:10D61B | INV_SkyrimCloudShape01 | 18,3 | true | true | false |
| 0:10D61C | INV_SkyrimCloudDistant06_O | 19,2 | true | true | false |
| 0:10D61D | SkyrimCloudDistant01 | 20,3 | true | true | false |
| 0:10D61E | SkyrimCloudDistant03_50 | 20,5 | true | true | false |
| 0:10D61F | SkyrimCloudDistant03_50 | 21,2 | true | true | false |
| 0:10D620 | SkyrimCloudDistant02 | 22,2 | true | true | false |
| 0:10D621 | SkyrimCloudShape03_O | 21,3 | true | true | false |
| 0:10D622 | INV_SkyrimCloudDistant05 | 21,0 | true | true | false |
| 0:10D623 | INV_SkyrimCloudShape02_50 | 20,0 | true | true | false |
| 0:10D624 | INV_SkyrimCloudShape02_50 | 20,1 | true | true | false |
| 0:10D625 | INV_SkyrimCloudShape02_O_50 | 21,0 | true | true | false |
| 0:10D626 | INV_SkyrimCloudDistant06_50 | 22,0 | true | true | false |
| 0:10D627 | INV_SkyrimCloudShape02_O_50 | 22,1 | true | true | false |
| 0:10D628 | INV_SkyrimCloudShape02_O_50 | 17,-9 | true | true | false |
| 0:10D629 | INV_SkyrimCloudShape02_O_50 | 18,-7 | true | true | false |
| 0:10D62A | SkyrimCloudShape03 | 17,-11 | true | true | false |
| 0:10D62B | SkyrimCloudShape03 | 21,-11 | true | true | false |
| 0:10D62C | INV_SkyrimCloudShape04_50 | 22,-10 | true | true | false |
| 0:10D62D | INV_SkyrimCloudShape03_50 | 21,-10 | true | true | false |
| 0:10D62E | INV_SkyrimCloudDistant03 | 22,-9 | true | true | false |
| 0:10D62F | INV_SkyrimCloudDistant04_50 | 21,-9 | true | true | false |
| 0:10D630 | INV_SkyrimCloudShape02_50 | 21,-9 | true | true | false |
| 0:10D631 | INV_SkyrimCloudShape02_50 | 18,-8 | true | true | false |
| 0:10D632 | SkyrimCloudShape02 | 18,-8 | true | true | false |
| 0:10D633 | INV_SkyrimCloudDistant04 | 19,-6 | true | true | false |
| 0:10D634 | INV_SkyrimCloudDistant03 | 18,-3 | true | true | false |
| 0:10D635 | INV_SkyrimCloudShape01_O | 18,-5 | true | true | false |
| 0:10D636 | INV_SkyrimCloudShape01 | 18,-5 | true | true | false |
| 0:10D637 | INV_SkyrimCloudShape01 | 23,-10 | true | true | false |
| 0:10D638 | INV_SkyrimCloudShape01_50 | 23,-9 | true | true | false |
| 0:10D639 | INV_SkyrimCloudShape01_50 | 24,-10 | true | true | false |
| 0:10D6B0 | INV_SkyrimCloudShape04_O_50 | 24,-8 | true | true | false |
| 0:10D6B1 | INV_SkyrimCloudShape03_O_50 | 23,2 | true | true | false |
| 0:10D6B2 | INV_SkyrimCloudShape05 | 22,4 | true | true | false |
| 0:10D6B3 | INV_SkyrimCloudShape06 | 22,4 | true | true | false |
| 0:10D6B4 | INV_SkyrimCloudShape06 | 23,5 | true | true | false |
| 0:10D6B5 | SkyrimCloudShape01_50 | 23,6 | true | true | false |
| 0:10D6B6 | SkyrimCloudShape02_50 | 23,7 | true | true | false |
| 0:10D6B7 | SkyrimCloudDistant04_O | 20,5 | true | true | false |
| 0:10D6B8 | SkyrimCloudShape02 | 19,6 | true | true | false |
| 0:10D6B9 | SkyrimCloudShape02_50 | 20,6 | true | true | false |
| 0:10D6BA | SkyrimCloudShape02_50 | 21,6 | true | true | false |
| 0:10D6BB | INV_SkyrimCloudShape03 | 21,6 | true | true | false |
| 0:10D6BC | SkyrimCloudDistant03 | 17,5 | true | true | false |
| 0:10D6BD | SkyrimCloudDistant03 | 16,5 | true | true | false |
| 0:10D6BE | INV_SkyrimCloudDistant03 | 18,6 | true | true | false |
| 0:10D6BF | INV_SkyrimCloudShape03 | 17,5 | true | true | false |
| 0:10D6C0 | INV_SkyrimCloudShape02 | 17,7 | true | true | false |
| 0:10D6C1 | INV_SkyrimCloudShape02 | 19,6 | true | true | false |
| 0:10D6C2 | INV_SkyrimCloudShape03_50 | 19,10 | true | true | false |
| 0:10D6C3 | INV_SkyrimCloudShape03_O | 16,11 | true | true | false |
| 0:10D6C4 | SkyrimCloudShape05_O | 16,11 | true | true | false |
| 0:10D6C5 | SkyrimCloudShape05_O | 16,11 | true | true | false |
| 0:10D6C6 | INV_SkyrimCloudDistant01 | 16,15 | true | true | false |
| 0:10D6C7 | INV_SkyrimCloudDistant02_O_50 | 14,14 | true | true | false |
| 0:10D6C8 | INV_SkyrimCloudShape01 | 15,13 | true | true | false |
| 0:10D6C9 | INV_SkyrimCloudShape01 | 12,15 | true | true | false |
| 0:10D6CA | INV_SkyrimCloudShape01 | 13,15 | true | true | false |
| 0:10D6CB | INV_SkyrimCloudDistant06 | 13,13 | true | true | false |
| 0:10D6CC | INV_SkyrimCloudDistant06 | 17,14 | true | true | false |
| 0:10D6CD | INV_SkyrimCloudShape01_O_50 | 15,13 | true | true | false |
| 0:10D6CE | INV_SkyrimCloudDistant04_O | 21,14 | true | true | false |
| 0:10D6CF | INV_SkyrimCloudDistant03_50 | 20,14 | true | true | false |
| 0:10D6D0 | INV_SkyrimCloudShape04 | 19,13 | true | true | false |
| 0:10D6D1 | INV_SkyrimCloudShape03 | 21,13 | true | true | false |
| 0:10D6D2 | INV_SkyrimCloudDistant05 | 24,14 | true | true | false |
| 0:10D6D3 | INV_SkyrimCloudShape01 | 23,14 | true | true | false |
| 0:10D6D4 | INV_SkyrimCloudShape01 | 23,13 | true | true | false |
| 0:10D6D5 | INV_SkyrimCloudShape01 | 24,14 | true | true | false |
| 0:10D6E5 | SkyrimCloudDistant03_50 | 24,16 | true | true | false |
| 0:10D6E6 | SkyrimCloudShape01_O_50 | 24,17 | true | true | false |
| 0:10D6E7 | SkyrimCloudShape01_O_50 | 23,15 | true | true | false |
| 0:10D6E8 | SkyrimCloudShape02_O | 23,15 | true | true | false |
| 0:10D6E9 | SkyrimCloudShape02_O | 24,15 | true | true | false |
| 0:10D6EA | SkyrimCloudDistant04 | 21,16 | true | true | false |
| 0:10D6EB | SkyrimCloudDistant05_50 | 21,17 | true | true | false |
| 0:10D6EC | INV_SkyrimCloudDistant02_50 | 21,16 | true | true | false |
| 0:10D907 | SkyrimCloudShape03_O_50 | -6,-19 | true | true | false |
| 0:10D908 | SkyrimCloudShape04_50 | -7,-19 | true | true | false |
| 0:10D909 | INV_SkyrimCloudShape03_O_50 | -6,-19 | true | true | false |
| 0:10D90A | INV_SkyrimCloudShape04_O | -7,-19 | true | true | false |
| 0:10D915 | INV_SkyrimCloudDistant01_O | -3,-12 | true | true | false |
| 0:10D916 | INV_SkyrimCloudDistant02 | -3,-11 | true | true | false |
| 0:10D917 | SkyrimCloudDistant06 | -3,-10 | true | true | false |
| 0:10D918 | SkyrimCloudDistant05 | -3,-9 | true | true | false |
| 0:10D919 | INV_SkyrimCloudDistant02_O | 3,-9 | true | true | false |
| 0:10D91A | SkyrimCloudDistant02_O | 2,-9 | true | true | false |
| 0:10D91B | SkyrimCloudDistant02_O | -14,-11 | true | true | false |
| 0:10D91C | INV_SkyrimCloudDistant02_O | -14,-11 | true | true | false |
| 0:10D91D | INV_SkyrimCloudShape05_O | -14,-10 | true | true | false |
| 0:10D940 | INV_SkyrimCloudDistant01_O_50 | 17,-18 | true | true | false |
| 0:10D941 | INV_SkyrimCloudDistant02_50 | 17,-18 | true | true | false |
| 0:10DA14 | INV_SkyrimCloudDistant05_O_50 | 15,17 | true | true | false |
| 0:10DA15 | INV_SkyrimCloudDistant05 | 15,16 | true | true | false |
| 0:10DA16 | SkyrimCloudShape05 | 13,15 | true | true | false |
| 0:10DA17 | SkyrimCloudShape03_O_50 | 14,15 | true | true | false |
| 0:10DA18 | INV_SkyrimCloudShape04 | 17,16 | true | true | false |
| 0:10DA68 | SkyrimCloudShape04 | 10,18 | true | true | false |
| 0:10DA6A | SkyrimCloudShape02_50 | 12,16 | true | true | false |
| 0:10DA6B | SkyrimCloudShape02_50 | 10,16 | true | true | false |
| 0:10DA6C | SkyrimCloudShape03_50 | 7,16 | true | true | false |
| 0:10DA6D | SkyrimCloudDistant04 | 2,15 | true | true | false |
| 0:10DA6E | SkyrimCloudDistant04_O_50 | 1,16 | true | true | false |
| 0:10DA6F | INV_SkyrimCloudDistant06_50 | 5,14 | true | true | false |
| 0:10DA70 | INV_SkyrimCloudShape02_O | 3,15 | true | true | false |
| 0:10DA71 | INV_SkyrimCloudShape02_O | 4,15 | true | true | false |
| 0:10DA72 | INV_SkyrimCloudShape02_O | 3,13 | true | true | false |
| 0:10DA73 | INV_SkyrimCloudShape02_O | -1,14 | true | true | false |
| 0:10DA74 | SkyrimCloudDistant03 | 0,14 | true | true | false |
| 0:10DA75 | SkyrimCloudDistant03 | -2,13 | true | true | false |
| 0:10DA76 | SkyrimCloudDistant01 | -2,11 | true | true | false |
| 0:10DA77 | SkyrimCloudDistant04_50 | -2,13 | true | true | false |
| 0:10DA78 | SkyrimCloudDistant04_50 | -1,11 | true | true | false |
| 0:10DA79 | INV_SkyrimCloudDistant06_O | 0,11 | true | true | false |
| 0:10E056 | INV_SkyrimCloudDistant03_O | 15,-20 | true | true | false |
| 0:10E05B | INV_SkyrimCloudShape01 | 15,-20 | true | true | false |
| 0:10E0C7 | INV_SkyrimCloudShape02 | 12,-19 | true | true | false |
| 0:10E0C8 | INV_SkyrimCloudShape03 | 13,-19 | true | true | false |
| 0:10E0C9 | SkyrimCloudShape01 | 15,-21 | true | true | false |
| 0:10E275 | INV_SkyrimCloudDistant02_50 | 17,-17 | true | true | false |
| 0:10E276 | INV_SkyrimCloudDistant02_50 | 17,-12 | true | true | false |
| 0:10E285 | INV_SkyrimCloudDistant02 | 13,-22 | true | true | false |
| 0:10E28C | SkyrimCloudDistant01 | -16,-22 | true | true | false |
| 0:10E293 | SkyrimCloudDistant02_50 | -15,-21 | true | true | false |
| 0:10E297 | INV_SkyrimCloudDistant01 | -8,-11 | true | true | false |
| 0:10E298 | INV_SkyrimCloudDistant01 | -3,-9 | true | true | false |
| 0:10E299 | INV_SkyrimCloudDistant02 | -11,-10 | true | true | false |
| 0:10E29A | INV_SkyrimCloudDistant02 | -11,-11 | true | true | false |
| 0:10E29B | SkyrimCloudDistant01_50 | -11,-11 | true | true | false |
| 0:10E29C | SkyrimCloudDistant02_50 | -11,-11 | true | true | false |
| 0:10E29D | INV_SkyrimCloudDistant02 | 18,-12 | true | true | false |
| 0:10E29E | INV_SkyrimCloudDistant02 | 17,-12 | true | true | false |
| 0:10E29F | INV_SkyrimCloudDistant01 | 15,-14 | true | true | false |
| 0:10E2A0 | INV_SkyrimCloudDistant02_50 | 17,-13 | true | true | false |
| 0:10E2A1 | INV_SkyrimCloudDistant01_50 | 14,-16 | true | true | false |
| 0:10E2A2 | INV_SkyrimCloudDistant02_50 | 14,-16 | true | true | false |
| 0:10E2A3 | INV_SkyrimCloudDistant02_50 | 13,-14 | true | true | false |
| 0:10E2A4 | INV_SkyrimCloudDistant01_O_50 | 14,-13 | true | true | false |
| 0:10E2A5 | INV_SkyrimCloudDistant02_50 | 14,-12 | true | true | false |
| 0:10E2A6 | INV_SkyrimCloudDistant02 | 13,-11 | true | true | false |
| 0:10E2AC | INV_SkyrimCloudDistant01_50 | -2,-22 | true | true | false |
| 0:10E2AD | INV_SkyrimCloudDistant02_50 | -1,-22 | true | true | false |
| 0:10E2AE | INV_SkyrimCloudDistant01 | -1,-18 | true | true | false |
| 0:10E2AF | SkyrimCloudDistant01 | -1,-18 | true | true | false |
| 0:10E2B0 | SkyrimCloudDistant02_50 | 5,-14 | true | true | false |
| 0:10E2B1 | SkyrimCloudDistant01 | 7,-11 | true | true | false |
| 0:10E2B2 | SkyrimCloudDistant02 | 6,10 | true | true | false |
| 0:10E8BE | INV_SkyrimCloudDistant02_O_25 | 2,-9 | true | true | false |
| 0:10EBE5 | SkyrimCloudDistant02_O | 12,-22 | true | true | false |
| 0:10EBE6 | SkyrimCloudShape02 | 13,-22 | true | true | false |
| 0:10EBE7 | SkyrimCloudDistant02_50 | 11,-21 | true | true | false |
| 0:10EBE8 | SkyrimCloudDistant03_O | 12,-21 | true | true | false |
| 0:10F169 | SkyrimCloudDistant01 | 11,-9 | true | true | false |
| 0:10F16A | SkyrimCloudDistant04_50 | 10,-9 | true | true | false |
| 0:10F16B | INV_SkyrimCloudDistant04_O | 14,-10 | true | true | false |
| 0:10F16C | INV_SkyrimCloudDistant04_O_50 | 12,-8 | true | true | false |
| 0:10F16D | INV_SkyrimCloudDistant05_50 | 9,-10 | true | true | false |
| 0:10F16E | INV_SkyrimCloudShape01_O | 10,-9 | true | true | false |
| 0:10F16F | INV_SkyrimCloudDistant06_O_50 | 14,-9 | true | true | false |
| 0:10F170 | INV_SkyrimCloudDistant03_O | 16,-12 | true | true | false |
| 0:10F567 | DragonPerchTower | 0,-4 | true | true | false |
| 0:10FF37 | DragonPerchRockL02 | 14,-12 | true | true | false |

## Memory measurement

The original PRF-35 run passed its timing and gameplay checks, with Godot
peak VmRSS 12,072,480 KiB. With worldspace full-model residency, the same
one-test, one-thread run passed those checks and peaked at 12,302,212 KiB.
The observed increase was 229,732 KiB, about 224 MiB. Both runs exceed the
8 GB acceptance limit. Samples came from the actual Godot child process
started by each cargo test, at 50 ms intervals.

The LOD cache diagnostic found 495 entries holding 199,621,080 bytes of
geometry. That cache does not explain the memory excess. The heap probe
reported about 6.6 GB allocated and 3.9 GB free in glibc arenas. Returning
free pages lowered peak VmRSS to 10,405,212 KiB and still exceeded 8 GB.
Fixing the glibc mmap threshold at 128 KiB peaked at 10,100,196 KiB.
Limiting glibc to two arenas with a 32 KiB mmap threshold peaked at
9,161,776 KiB and hit the 300 second launch timeout. These diagnostic
settings are not part of the implementation. The 8 GB limit remains
unmet, including by the unchanged baseline. RND-40 remains wip.

The final untuned PRF-35 run passed one test in 209.02 seconds. Godot
peak VmRSS was 12,439,432 KiB, or 12,737,978,368 bytes, sampled from
the started Godot process. This is 366,952 KiB, about 358 MiB, above
the baseline. The earlier paired measurement added about 224 MiB.
`target/full-lod-prf-final-rss.json` holds the final process samples.
The 8 GB acceptance check failed.

## Verification

## Historical RND-40 verification

The host library suite passed 434 tests with installed inputs unset.
The five focused selection and sharing tests passed, including three
installed tests which returned early without those inputs. With installed
inputs, the survey and preparation tests each passed. The live rendering
test passed with 785 of 785 eligible drawable Tamriel references and all
251 near the gate drawn. Returning to Whiterun retained zero Tamriel
full models. The final live rendering run passed in 33.12 seconds.
Marker-only models remained invisible.

`cargo fmt -p soe-godot-host`,
`cargo clippy -p soe-godot-host --all-targets -- -D warnings` and
`cargo test -p soe-godot-host --lib` passed. The release build and headless
Godot import passed. No optional gate tools were run, following the task
instruction to omit `tools/lint.sh`.

The default Xvfb gate play path returned 101, with six harness tests
passing and the installed test failing after 21.27 seconds. It stopped
at the Hulda approach before the exterior. The observations below come
from `target/tst_19/gate/stdout.log`. A failed observation stays failed
even if an earlier observation passed. No assertion or expected-failure
policy was changed. The backtrace used gdb but attached to the timeout
wrapper process, so it did not capture the Godot thread stacks.

| Check | Result | Observed data |
| --- | --- | --- |
| window | Pass | {"DISPLAY": ":99", "display": "X11", "mode": "gate", "size": "(640, 360)"} |
| capture | Pass | {"path": "startup.png"} |
| route.walk | Fail | {"destination": "(176.0, -566.0, 69.2457)", "distance": 340.804473876953, "path_points": 12, "position": "(145.4008, -592.563, 68.24887)", "seconds": 8.122} |
| route.hulda | Not observed | Run stopped at Hulda |
| route.container | Not observed | Run stopped at Hulda |
| route.loose | Not observed | Run stopped at Hulda |
| route.hearth | Not observed | Run stopped at Hulda |
| route.exit | Not observed | Run stopped at Hulda |
| route.market | Not observed | Run stopped at Hulda |
| route.gate | Not observed | Run stopped at Hulda |
| route.return | Not observed | Run stopped at Hulda |
| route.repeat | Not observed | Run stopped at Hulda |
| duration | Not observed | Run stopped at Hulda |
| npc.drawn | Fail | {"drawn": 0, "in_view": 1} |
| npc.head_hair | Fail | {"hairs": 0, "heads": 0, "in_view": 1} |
| npc.pose | Fail | {"changed": 0, "interval_s": 1.0, "pairs": 0} (some earlier poses changed) |
| frame.max_ms | Pass | {"frames": 0, "over_50_ms": 0, "warmup_s": 10.0, "worst_ms": 0.0} (warmup only, zero measured frames) |
| watchdog.max_ms | Fail | {"limit_ms": 2000, "stale_ms": 2007} |
| fire.additive | Not observed | Run stopped at Hulda |
| fire.hull | Not observed | Run stopped at Hulda |
| terrain.textures | Not observed | Run stopped at Hulda |
| object.white | Not observed | Run stopped at Hulda |
| object.height | Not observed | Run stopped at Hulda |
| doors.drawn | Pass | {"doors": 6, "missing": 0} |
| doors.states | Not observed | Run stopped at Hulda |
| compass.offset | Not observed | Run stopped at Hulda |
| menu.inventory_theme | Not observed | Run stopped at Hulda |
| menu.container_theme | Not observed | Run stopped at Hulda |
| menu.dialogue_theme | Not observed | Run stopped at Hulda |
| audio.stone | Not observed | Run stopped at Hulda |
| audio.grass | Not observed | Run stopped at Hulda |
| gate_display | Pass | {"DISPLAY": ":99"} |

## Visibility decision, RND-41

Selection reads winning REFR DATA and XSCL, its base OBND, and the existing
Full LOD or STAT Never Fades flags before requesting any NIF. All eight
rotated and scaled bounds corners contribute to the world AABB.
Bounds use the signed 16-bit XYZ minimum and maximum declared by xEdit's
`wbObjectBounds` in `wbDefinitionsCommon.pas` and UESP `OBND_Field.wikitext`.
Missing, zero or malformed bounds use a conservative 4096-unit cube at the placement.
The diagonal must project to at least two pixels at a reference 720-pixel
height and 75-degree vertical field of view. This normalized screen threshold
is independent of actual window resolution and camera heading.

A ray to any corner or the top centre clearing terrain admits the model.
Joined resident BTR triangles supply a spatial height index per rendered
surface. The old terrain LOD streamer retained meshes, not a ready CPU
heightmap. The index is built alongside the rendered mesh, with no asset read.
Water and unclassified surfaces supply no terrain blockers.
Resident LAND heights replace masked BTR cells using the rendered SW to NE
triangle diagonal. Hidden BTR geometry cannot supply a near-cell blocker.
Sight lines sample every 128 units, capped at 8192 samples per ray, and lower
terrain by 140 units to retain
grazing silhouettes. Missing triangle coverage supplies no blocker. This is
conservative sampling against rendered LOD, not proof of exact LAND horizons.

Selection runs after 512 units of eye movement, a worldspace change, terrain
replacement or a near-grid ownership change. Idle frames do not repeat terrain
queries. The camera rig supplies first-person and third-person eye placement.
Already admitted references retain a 1.5-pixel threshold, giving a release
reach one third larger than admission. Three hidden observations and thirty
update frames are required for release. Heading changes never evict a model.
Once three hidden observations have accumulated, the frame debounce finishes
even when the player stops, without repeating terrain queries.
At most eight newly admitted references prepare per update, nearest first
with FormID tie breaks. Further admission batches drain the retained visible
demand without repeating sight lines. Movement or terrain changes replace
pending demand before another model is prepared.
Admitted demand uses live enable state, so an authored disabled flag does not
reject a reference that the game enabled.
All paths preparing a large-world persistent cell directly return no models.
Generated object LOD still excludes persistent full-model candidates.

Distant static placements share MultiMesh geometry and existing material and
texture caches. Selected CPU surfaces survive demand updates. Worldspace
replacement releases instances, selected surfaces, height samples and selector
state and prunes material and prepared texture caches. Logical reference
ownership stays in the load-order database. No saved section, dependency or
per-reference node is added.

Cell occluders and the project's occlusion setting default to enabled.
Occluders use exact opaque source triangles and placement transforms, with
no hull or expanded box. Water, cutouts, effects, refraction, skinning and
animation are excluded, including TREE_ANIM vertex motion in the material
shader. Single-sided surfaces are excluded because Godot treats occluder
triangles as two-sided. A closed shell can still have invisible back faces
when the camera is inside it. A source pose, enable or destruction change
removes its old cell occluder. Missing blockers reduce culling rather than
hide visible geometry.

Joined terrain LOD supplies server-side occluders with the same triangles and
holes, lowered two metres beneath its rendered surface. The join mask retains
loaded LAND holes and recreates occluders at handoff. Water is excluded.
Occluder RIDs are freed with the rendered block. Instance AABBs are unchanged,
so Godot keeps any partly visible bound. Large batches retain large bounds,
which can reduce culling effectiveness.

The comparison reports were checked against local Mudcrab PR 210 demand and
shared admission, PR 211 reservations, and PR 223 and 225 room visibility.
Nearby demand before loads, bounded admission and fallback to visible informed
this adapter, credited in distant_visibility.rs. PR 211's reservation ledger
and adaptive stage controller were read. Their Bevy upload and disposal
signals do not map to this synchronous server-RID path, which uses a fixed
admission cap and measured process memory. No Mudcrab code was copied.
The research culling and chef-201-vs-drawn-bridge notes separate API setup,
CPU readiness and actual drawn evidence. Their Bevy render retry and room graph
do not fit this server-RID path and were not ported. Context7 was queried for
Godot 4.4.1 and occluder APIs, then checked against the versioned
[occlusion guide](https://docs.godotengine.org/en/4.4/tutorials/3d/occlusion_culling.html)
and [RenderingServer API](https://docs.godotengine.org/en/4.4/classes/class_renderingserver.html).

The synthetic checks cover ridge rejection, a top clearing the ridge, missing
coverage, scale and rotation, release reach, hidden debounce, idle frames,
nearest admission, shared meshes and open-sheet occluder rejection. A Godot
fixture compares draw calls and exposed red pixels with occlusion off and on.
The installed Whiterun check uses Xvfb :99 at 640 by 360, samples actual Godot
VmRSS through the tracked process tree, and has a predeclared six GiB peak
limit. It records flagged-model counts and draw calls at the gate and after
returning inside Whiterun, with a mountain-over-terrain capture and control.
The ten pure selector and terrain checks passed. On Godot 4.4.1 under llvmpipe,
the isolated fixture reduced draw calls from three to two and retained
266 of 266 exposed red pixels. The RND-07 adversarial fixtures retain their
source-order, size-order and placement assertions, with two-sided opaque
materials to match their open triangle geometry. The open single-sided
rejection test checks the new conservative exclusion.

Two installed observations stopped before completing the scene. Their actual
Godot VmRSS peaks were 11.256 and 11.324 GiB. The latter reached 325 drawn
flagged references out of 785 eligible drawable Tamriel placements. These
incomplete runs do not establish the draw-call comparison, city release or
landmark capture. Old capture files are not evidence for a newer run.
The next observation uses MALLOC_ARENA_MAX=2, MALLOC_TRIM_THRESHOLD_=65536 and
LP_NUM_THREADS=4, alongside the run wrapper's installed slot and memory floor.
Those are test-host settings, not changes to the project or the six GiB check.

## Continuation of the timed attempt, 2026-10-13

This work continues detached `dfa8db6f2` in the lead-play-visibility scratch
worktree. Pending admissions retain the nearest-first visible IDs instead of
repeating every terrain sight line after each eight-model batch. A terrain
revision replaces pending demand. Deferred release completes at rest after
three hidden observations and thirty update frames. An explicit zero XSCL
produces zero projected size, while an absent XSCL retains the reader's unit
scale default.

The local xEdit MSTT definition, lines 5333 to 5351, names Never Fades at
header bit 2. Persistent MSTT models now use the same visible-demand path as
STAT models. The ownership fixture admits the new MSTT candidate, retains
the MISC exclusion, and still prepares zero models from a direct large-world
persistent-cell load. Cell blockers require two-sided opaque materials.
Closed topology alone cannot prove that a surface blocks from both sides.
TREE_ANIM shader motion also excludes a surface from blocker selection.
RND-07's placement and index fixtures and the skinning and RND-23 animation
fixtures now use two-sided opaque materials. Their assertions stay unchanged
and still isolate those contracts from the one-sided exclusion.

Fourteen selector and terrain tests and three occluder regressions passed.
The MSTT ownership fixture passed separately. The Godot 4.4.1 fixture on
Xvfb :99 reduced draw calls from three to two and retained all 266 exposed
pixels. Every installed observation now keeps a unique `target/full-lod/run-*`
directory and prints its path, so an earlier capture cannot fill a newer run.
Memory telemetry is emitted before the draw-call and landmark captures.
The six GiB check, landmark pixel threshold and city-release assertions stay
unchanged. No dependency, saved section or production per-reference node was
added.

Context7 and Serena were unavailable in this continuation. The existing API
adapter was checked against the local Godot documentation and the versioned
Godot 4.4 OccluderInstance3D and occlusion guide. The earlier Context7 query
remains recorded above. The continuation rebuilt the release extension through
the run's cargo wrapper. Installed observations used Xvfb :99, the wrapper's
installed slot and the allocator and llvmpipe settings recorded above. No
hand-started Godot process or Skyrim process was used.

The completed `target/full-lod/run-5D9ucg` observation measured the following
with Godot 4.4.1 and llvmpipe at 640 by 360:

| Location | Loaded flagged models | Distant models | Draw calls off | Draw calls on |
| --- | ---: | ---: | ---: | ---: |
| Whiterun gate | 325 | 324 | 3989 | 1839 |
| Inside Whiterun | 0 | 0 | 2889 | 1010 |

The gate had 785 eligible drawable Tamriel placements and 251 near-grid
flagged references. Admission is 325, rather than the whole-world eligible
set. The city has no authored flagged models in this fixture. Returning to
Whiterun released all Tamriel full models and distant ownership. Gate
occluders comprised two cell meshes and 261 terrain blocks, with 280150820
bytes of resident height samples and 3547564 bytes of distant CPU geometry.
The city retained two cell occluders and 260 parent terrain blocks.

Godot exited normally, but the Rust test returned 101 because its measured
peak VmRSS was 12.373745 GiB, above the unchanged six GiB limit. These are
whole-process bytes, not an estimate of model payload. The observed budget
failure remains open. RND-40 and RND-41 remain `wip`, with no completed
evidence claim. An earlier observation peaked at 13.076820 GiB. Its capture's
1698-pixel result was a false positive from foreground geometry, found by
inspecting the PNG. That image does not prove a landmark over a ridge.
The final capture uses RND-27's separate outside-gate camera and requires
clear sky across the control image's top edge. It passes with 1308 pixels
against the unchanged 500-pixel threshold. Both its landmark and terrain
control PNGs were inspected, showing mountain geometry above the foreground
ridge. The gameplay camera is restored before city measurements.

## Scratch preview merge, 2026-10-14

The merge retains both residency and visibility test modules. The original
`full_lod_tests.rs` and `persistent_full_lod_test.gd` are unchanged. The incoming
checks live in `full_lod_visibility_tests.rs` and
`persistent_full_lod_visibility_test.gd`, with all their assertions retained.
The older disjoint-grid and whole-world preparation checks conflict with the
visibility decision and remain executable rather than being weakened.

Local `../engine-refs/uesp-modfileformat/REFR.wikitext` names the Full LOD flag.
`../engine-refs/TES5Edit/Core/wbDefinitionsTES5.pas` names Never Fades on STAT
and MSTT. Those format definitions select the full model, not an unconditional
whole-world Godot allocation. The requested visibility implementation keeps
near models, conservative visible distant demand and logical record ownership.
The older residency assertions describe the superseded preparation policy.

## Preview contract reconciliation, 2026-10-14

The legacy Full LOD checks now follow the visibility decision. Flagged models
inside the near grid share physical presentation. The persistent world batch
starts empty and selected distant demand prepares at most eight additions
per update. Installed checks require every admitted drawable model, retain
marker exclusion and release on return, and reject whole-world preparation.
The separate visibility suite retains screen-size, terrain sight-line,
hysteresis, conservative occlusion and landmark image checks. The legacy
Godot fixture waits for the authored door fades before inspecting resources.
Its former requirement to draw every eligible model throughout Tamriel was
the old contract, not the governing visibility behavior.
