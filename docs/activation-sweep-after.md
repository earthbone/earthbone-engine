# Activation sweep

CK contracts and installed records, no captured vanilla runtime oracle

A pass covers the stated observation. Unknown script effects and missing host runs fail.

| Base | pass | wrong outcome | no outcome | error |
| --- | ---: | ---: | ---: | ---: |
| ACTI | 0 | 85 | 186 | 495 |
| ALCH | 28 | 0 | 0 | 226 |
| AMMO | 1 | 0 | 0 | 5 |
| ARMO | 1 | 1 | 0 | 30 |
| BOOK | 28 | 2 | 0 | 47 |
| CONT | 28 | 40 | 0 | 140 |
| DOOR | 1 | 28 | 1 | 50 |
| FLOR | 11 | 0 | 3 | 105 |
| FURN | 30 | 10 | 26 | 217 |
| INGR | 2 | 0 | 0 | 23 |
| KEYM | 0 | 1 | 0 | 0 |
| MISC | 105 | 0 | 0 | 655 |
| NPC_ | 0 | 0 | 57 | 80 |
| SLGM | 19 | 6 | 0 | 91 |
| TREE | 25 | 0 | 17 | 331 |
| WEAP | 11 | 1 | 0 | 28 |

| Failure cause | References |
| --- | ---: |
| activation_event | 1 |
| animation_wait_not_completed | 19 |
| dialogue_requires_host | 57 |
| furniture_entry_missing | 62 |
| harvest_default_missing | 48 |
| headless_animation_error | 3 |
| host_activation_event | 27 |
| host_animation_no_motion | 246 |
| host_behavior_binding_error | 82 |
| host_menu_missing | 57 |
| host_papyrus_error | 1785 |
| host_run_incomplete | 616 |
| no_observable_effect | 32 |
| papyrus:CWDisableDuringSiegeScript.OnCellLoad | 25 |
| papyrus:CWFieldCOScript.OnLoad | 10 |
| papyrus:CWLockDuringSiegeScript.OnCellLoad | 25 |
| papyrus:CWMissionGeneratorTriggerScript.OnCellAttach | 15 |
| papyrus:DLC1ArkngthamzDestructionSCRIPT.OnActivate | 3 |
| papyrus:DLC2SoulStealScript.MiraakDisappears | 23 |
| papyrus:DialogueSwapGovernmentAliasesScript.TestForSwap | 2487 |
| papyrus:FXCameraAttachScript.OnUnLoad | 2 |
| papyrus:MQ106BurialMoundScript.OnUpdate | 96 |
| papyrus:MagicTrap.fireTrap | 15 |
| papyrus:ObjectReference.IsInLocation | 15 |
| papyrus:PF_WhiterunSigurdCarryFirewoo_000CDE5D.Fragment_0 | 2 |
| papyrus:PF_WhiterunSigurdCarryFirewoo_000CDE5D.Fragment_2 | 2 |
| papyrus:StockadeBarricade01ActivatorScript.OnLoad | 1 |
| papyrus:Survival_MainScript.ModeCanBeEnabled | 1489 |
| papyrus:TestSetAVLeverScript.ActivateNow | 1 |
| papyrus:TrapBase.onActivate | 16 |
| papyrus:TrapFallingOilLampColLayer.switchToFalling | 10 |
| papyrus:TrapPoisonGas.ResolveGasActivation | 4 |
| papyrus:TrapTriggerBase.onActivate | 15 |
| papyrus:TrapTriggerHinge.onBeginState | 2 |
| papyrus:TutorialPlayerScript.OnUpdateGameTime | 192 |
| papyrus:WETriggerScript.StartEncounter | 16 |
| papyrus:WETriggerScript.getMyHoldFactionControllerMarker | 16 |
| papyrus:WIFunctionsScript.RegisterDragonAttack | 8 |
| papyrus:WeaponRackActivateSCRIPT.HandleWeaponPlacement | 27 |
| papyrus:WeaponRackActivateSCRIPT.onActivate | 27 |
| papyrus:defaultDisableOnResetIfLocWasCleared.onCellDetach | 1 |
| papyrus:defaultOwnZombie.MakeZombie | 15 |
| papyrus:defaultPlayerEnableDisableLinkedRef.onActivate | 1 |
| papyrus:dweSphereCenturionPortScript.onActivate | 5 |
| papyrus:dweSpiderCenturionPipeScript.onActivate | 5 |
| papyrus:dweSteamCenturionStructureScript.onActivate | 1 |
| papyrus_error | 2046 |
| script_or_animation_unverified | 337 |

Cell failures are counted separately from reference outcomes.

script_attachment_missing: 1 cells.
startup_papyrus_error: 68 cells.

Cells: 68. Required host visits: 45. Host complete: 37.

Fresh world per cell, sequential activation within each cell, three simulated seconds per reference.
The host uses the walkthrough activation entry point. Target selection rays and full vanilla script effects need separate oracles.
Headless transforms establish RID motion. They do not establish correct pixels or behavior graph event timing.
