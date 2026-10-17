# Activation sweep

CK contracts and installed records, no captured vanilla runtime oracle

A pass covers the stated observation. Unknown script effects and missing host runs fail.

| Base | pass | wrong outcome | no outcome | error |
| --- | ---: | ---: | ---: | ---: |
| ACTI | 0 | 92 | 183 | 482 |
| ALCH | 100 | 0 | 0 | 154 |
| AMMO | 3 | 0 | 0 | 3 |
| ARMO | 10 | 1 | 0 | 21 |
| BOOK | 28 | 2 | 0 | 47 |
| CONT | 33 | 40 | 0 | 122 |
| DOOR | 5 | 24 | 1 | 48 |
| FLOR | 0 | 0 | 47 | 70 |
| FURN | 63 | 10 | 30 | 178 |
| INGR | 9 | 0 | 0 | 16 |
| KEYM | 0 | 1 | 0 | 0 |
| MISC | 290 | 0 | 0 | 469 |
| NPC_ | 0 | 0 | 57 | 80 |
| SLGM | 39 | 6 | 0 | 71 |
| TREE | 0 | 0 | 95 | 161 |
| WEAP | 12 | 1 | 0 | 27 |

| Failure cause | References |
| --- | ---: |
| dialogue_requires_host | 57 |
| furniture_entry_missing | 10 |
| harvest_default_missing | 142 |
| host_animation_no_motion | 13 |
| host_papyrus_error | 13 |
| no_observable_effect | 34 |
| papyrus:CWDisableDuringSiegeScript.OnCellLoad | 4 |
| papyrus:CWFieldCOScript.OnLoad | 2 |
| papyrus:CWLockDuringSiegeScript.OnCellLoad | 4 |
| papyrus:CWMissionGeneratorTriggerScript.OnCellAttach | 2 |
| papyrus:DLC1ArkngthamzDestructionSCRIPT.OnActivate | 3 |
| papyrus:DLC2SoulStealScript.MiraakDisappears | 16 |
| papyrus:DialogueSwapGovernmentAliasesScript.TestForSwap | 1867 |
| papyrus:ObjectReference.IsInLocation | 2 |
| papyrus:TestSetAVLeverScript.ActivateNow | 1 |
| papyrus:TrapFallingOilLampColLayer.switchToFalling | 10 |
| papyrus:TrapPoisonGas.ResolveGasActivation | 4 |
| papyrus:TrapTriggerBase.onActivate | 15 |
| papyrus:Tripwire.onBeginState | 15 |
| papyrus:TutorialPlayerScript.OnUpdateGameTime | 103 |
| papyrus:WETriggerScript.StartEncounter | 2 |
| papyrus:WETriggerScript.getMyHoldFactionControllerMarker | 2 |
| papyrus:WIFunctionsScript.RegisterDragonAttack | 8 |
| papyrus:WeaponRackActivateSCRIPT.HandleWeaponPlacement | 27 |
| papyrus:WeaponRackActivateSCRIPT.onActivate | 27 |
| papyrus:barredDoor.onActivate | 2 |
| papyrus:defaultDisableOnResetIfLocWasCleared.onCellDetach | 1 |
| papyrus:defaultOwnZombie.MakeZombie | 2 |
| papyrus:defaultPlayerEnableDisableLinkedRef.onActivate | 1 |
| papyrus:dweSphereCenturionPortScript.onActivate | 5 |
| papyrus:dweSpiderCenturionPipeScript.onActivate | 5 |
| papyrus:dweSteamCenturionStructureScript.onActivate | 1 |
| papyrus:trapRockfallEffects.onActivate | 5 |
| papyrus_error | 1949 |
| script_or_animation_unverified | 347 |

Cells: 51. Host complete: 1.

Fresh world per cell, sequential activation within each cell, three simulated seconds per reference.
The host uses the walkthrough activation entry point. Target selection rays and full vanilla script effects need separate oracles.
Headless transforms establish RID motion. They do not establish correct pixels or behavior graph event timing.

Initial baseline before strict state contracts, graph driver, ordered vanilla archives and added Whiterun exterior cells.
