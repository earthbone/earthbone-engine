# UI framework research

## Running mod SWF menus through Ruffle, UI-23

The UI-19 path converts the winning interface SWF art into Godot resources. It
does not execute ActionScript. UI-19 writes JPEXS output to an external,
content-addressed cache as reference material. The scripts are not copied into
the repository or run by the engine.

### Scaleform surface found in the source

The supplied SkyUI Community ActionScript tree contains 152 `.as` files. A
literal scan finds 470 `gfx.io.GameDelegate` operations, 337 `call` sites and
133 `addCallBack` sites. The scripts refer to five `gfx` namespace roots:
`io` (470 references), `ui` (140), `managers` (69), `events` (26), and
`controls` (8). `_global.gfxExtensions` is assigned in three files and
`_global.gfxPlayer` is referenced once.

The UI-19 converter recognizes these SWF tag families while building art and
timelines. They are ordinary SWF tags, not Scaleform-only tags:

| Purpose | Tags handled by UI-19 |
|---|---|
| Shapes and images | `DefineShape`, `DefineBits`, `DefineBitsJpeg2`, `DefineBitsJpeg3`, `DefineBitsLossless`, `JpegTables` |
| Text and fonts | `DefineEditText`, `DefineText`, `DefineText2`, `DefineFont2` |
| Display and timeline | `DefineSprite`, `PlaceObject`, `RemoveObject`, `ShowFrame`, `FrameLabel` |
| Symbols and scripts | `ExportAssets`, `ImportAssets`, `SymbolClass`, `DoAction`, `DoInitAction`, `DefineButton`, `DefineButton2` |
| Explicitly unsupported by UI-19 | `DefineMorphShape`, `DefineFont`, `DefineFont4`, and unknown tag codes, retained as unsupported tag IDs |

Ruffle PR 9090 adds `GFX` and `CFX` as uncompressed and zlib-compressed SWF
header signatures. It changes signature recognition only. It does not add
Scaleform ActionScript APIs, Skyrim's game delegate, or host callbacks. These
four signatures are file headers, not SWF tag codes.

### Ruffle gaps for Skyrim menus

Ruffle runs ActionScript 1 and 2, but a Skyrim menu also expects the host
services and Scaleform extensions named below. The Ruffle repository does not
provide the Skyrim `gfx` namespace. Each used extension needs either a
compatible implementation or a documented per-menu replacement. The `GameDelegate`
entries belong to the Skyrim host and need an engine bridge even if Ruffle
executes the surrounding script.

| Surface used by source | Examples | Required work |
|---|---|---|
| Skyrim host bridge | `gfx.io.GameDelegate.call`, `addCallBack` | Dispatch typed menu requests, deliver callbacks, preserve argument and callback order, and reject unknown operations clearly |
| Navigation constants | `gfx.ui.NavigationCode` including `TAB`, `ENTER`, directions, and gamepad buttons | Map host keyboard and controller input to the exact constants used by each menu |
| Focus management | `gfx.managers.FocusHandler.instance.setFocus`, `getFocus` | Maintain focus across lists, cards, dialogs, and menu transitions |
| Input adaptation | `gfx.managers.InputDelegate.instance.enableControlFixup`, `isGamepad`, `inputToNav` | Translate Godot input events and controller layouts into the navigation events expected by the SWF |
| Controls and events | `gfx.controls.Button`, `gfx.controls.ButtonGroup`, `gfx.controls.ScrollBar`, `gfx.events.EventDispatcher` | Supply compatible components and event ordering for scripts that construct or extend them |
| Scaleform text and mouse extensions | `TextField.SetText`, `MovieClip` platform/button helpers, mouse callbacks | Audit each used extension against Ruffle and implement the missing behavior without changing menu semantics |

This inventory is a source-level compatibility map, not a claim that stock
Ruffle lacks every similarly named Adobe Flash feature. The exact missing
method set must be checked against a pinned Ruffle revision during the first
prototype. Ruffle's current project description says ActionScript 1 and 2 are
supported, while Scaleform's own reference describes its extensions as
Scaleform-only APIs.

### GameDelegate calls by vanilla source class

The table lists literal call and callback names in the supplied Vanilla and
PauseMenu ActionScript directories. A class with several methods may
contribute to one menu. Shared classes are included so the bridge inventory
does not silently omit their calls. These names are case-sensitive.

| Source class | GameDelegate calls and callbacks |
|---|---|
| `Vanilla/BookBottomBar` | `ShowTakeButton` |
| `Vanilla/ButtonMapping` | `SetButtonMapping` |
| `Vanilla/ButtonTextArtHolder` | `GetButtonFromUserEvent` |
| `Vanilla/CharacterSelectHintButtons` | `PlaySound` |
| `Vanilla/Console` | `ExecuteCommand`, `HideComplete` |
| `Vanilla/CrossPlatformButtons` | `myLog` |
| `Vanilla/DialogueMenu` | `AdjustForPALSD`, `Cancel`, `CloseMenu`, `FadeDone`, `HideDialogueText`, `NotifyVoiceReady`, `PopulateDialogueList`, `SetSpeakerName`, `ShowDialogueList`, `ShowDialogueText`, `SkipText`, `StartHideMenu`, `TopicClicked` |
| `Vanilla/FaderMenu` | `FadeDone`, `InitFade`, `UpdateFade` |
| `Vanilla/HUDMenu` | `FadeOutChargeMeters`, `FadeOutStamina`, `FlashShoutMeter`, `HideSubtitle`, `RegisterHUDComponents`, `SetChargeMeterPercent`, `SetCompassAngle`, `SetCompassMarkers`, `SetCompassTemperature`, `SetCrosshairEnabled`, `SetCrosshairTarget`, `SetEnemyHealthPercent`, `SetEnemyHealthTargetPercent`, `SetHealthMeterPercent`, `SetLoadDoorInfo`, `SetLocationName`, `SetMagickaMeterPercent`, `SetShoutMeterPercent`, `SetStaminaMeterPercent`, `SetSubtitlesEnabled`, `ShowElements`, `ShowMessage`, `ShowNotification`, `ShowSubtitle`, `ShowTutorialHintText`, `StartMagickaMeterBlinking`, `StartStaminaMeterBlinking`, `ValidateCrosshair` |
| `Vanilla/LevelUpMenu` | `PlaySound`, `addHealth`, `addMagicka`, `addStamina` |
| `Vanilla/LoadingMenu` | `RequestLoadingText`, `RequestPlayerInfo` |
| `Vanilla/LockpickingMenu` | `SetLockInfo`, `SetPickMinMax`, `ToggleDebugMode`, `UpdateLockAngle`, `UpdatePickAngle`, `UpdatePickHealth`, `UpdateSweetSpot` |
| `Vanilla/MainSaveLoadList` | `PlaySound` |
| `Vanilla/MessageBox` | `PlaySound`, `buttonPress`, `setButtons`, `setIsCancellable`, `setIsVertical`, `setMessageText` |
| `Vanilla/ObjectiveText` | `PlaySound` |
| `Vanilla/QuestNotification` | `PlaySound` |
| `Vanilla/RaceSexPanels` | `ChangeName`, `ConfirmDone`, `FadeOut`, `HideLoadingIcon`, `PlaySound`, `SetAllowTextInput`, `SetCategoriesList`, `SetNameText`, `SetOptionSliders`, `SetRaceList`, `SetRaceText`, `ShowTextEntry`, `ShowTextEntryField`, `ShowVirtualKeyboard`, `ZoomPC`, `moveCategoriesUp` |
| `Vanilla/RaceWidePanel` | `PlaySound` |
| `Vanilla/SaveLoadPanel` | `CharacterSelected`, `ConfirmOKToLoad`, `DeleteSave`, `ForceStopSaveListLoading`, `IsOKtoLoad`, `PrepSaveGameScreenshot`, `ScreenshotReady`, `onFillCharacterListComplete`, `onSaveLoadBatchComplete` |
| `Vanilla/SleepWaitMenu` | `Cancel`, `OK`, `PlaySound` |
| `Vanilla/StartMenu` | `CONTINUE`, `ChangeUser`, `ConfirmContinue`, `ConfirmNewGame`, `CreationClub`, `DoDeleteSaveUISanityCheck`, `DoLoadDLCPlugins`, `DoneLoadingDLC`, `DownloadAll`, `EndPressStartState`, `FadeInMenu`, `FadeOutMenu`, `HELP`, `LoadDLC`, `MOD`, `NEW`, `ORBISDeleteSave`, `OnDeleteSaveUISanityCheck`, `OnDisabledLoadPress`, `OnPS5DataTransfer`, `OnSaveDataEventLoadCANCEL`, `OnSaveDataEventLoadSUCCESS`, `OpenMarketplace`, `PlaySound`, `PopulateCharacterList`, `QuitToDesktop`, `QuitToMainMenu`, `ShowGamerTagAndIcon`, `Sky10DLCPressed`, `StartLoadingDLC`, `StartState`, `currentState`, `fadeOutStarted`, `onProfileChange`, `onStartButtonProcessFinished`, `sendMenuProperties` |
| `Vanilla/StatsMenu` | `HideRing`, `MoveCamera`, `SetDescriptionCard`, `SetDirection`, `SetPerkCount`, `SetPlayerInfo`, `SetStatsMode`, `ShowLegendaryButtonHint`, `UpdateSkillList` |
| `Vanilla/TrainingMenu` | `Exit`, `Train` |
| `Vanilla/TutorialMenu` | `CloseMenu` |
| `Vanilla/UniformTimeMeter` | `PlaySound` |
| `PauseMenu/JournalBottomBar` | `PlaySound`, `RememberCurrentTabIndex`, `RequestPlayerInfo` |
| `PauseMenu/JournalSaveLoadList` | `PlaySound` |
| `PauseMenu/Quest_Journal` | `CloseMenu`, `HideMenu`, `PlaySound`, `RestoreSavedSettings`, `ShouldShowMod`, `ShowMenu`, `StartCloseMenu`, `onRightStickInput` |
| `PauseMenu/QuestsPage` | `PlaySound`, `RequestObjectivesData`, `RequestQuestsData`, `ShowTargetOnMap`, `ToggleQuestActiveStatus`, `ToggleShowMiscObjectives` |
| `PauseMenu/SettingsOptionItem` | `OptionChange`, `PlaySound` |
| `PauseMenu/StatsPage` | `PlaySound`, `updateStats` |
| `PauseMenu/SystemPage` | `BackOutFromLoadGame`, `CharacterSelected`, `FinishRemapMode`, `LOAD`, `LoadGame`, `ModManager`, `ORBISDeleteSave`, `OnSaveDataEventLoadCANCEL`, `OnSaveDataEventSaveCANCEL`, `OnSaveDataEventSaveSUCCESS`, `OpenKinectTuner`, `OptionChange`, `PlaySound`, `PopulateCharacterList`, `PopulateCreationClubTopics`, `PopulateHelpTopics`, `QuickSave`, `QuitToDesktop`, `QuitToMainMenu`, `RefreshSystemButtons`, `RequestAudioOptions`, `RequestCreationClubText`, `RequestDisplayOptions`, `RequestGameplayOptions`, `RequestHelpText`, `RequestInputMappings`, `RequestIsOnPC`, `ResetControlsToDefaults`, `SAVE`, `SaveControls`, `SaveGame`, `SaveSettings`, `SetRemoteDevice`, `SetSaveDisabled`, `SetVersionText`, `SettingsSaved`, `ShouldShowKinectTunerOption`, `StartRemapMode`, `UpdatePermissions`, `UseCurrentCharacterFilter` |

### Estimate and proposed work

These estimates assume the existing Godot runtime stays the host, the existing
UI-19 conversion remains the art path, and Ruffle is embedded behind a narrow
host interface. They are engineering estimates, not measurements. A prototype
must revise them before implementation is split into backlog rows.

| Piece | Estimate | Scope |
|---|---:|---|
| Pin Ruffle, build an embedded runtime, and render one static vanilla SWF | 1 to 2 weeks | Build integration, lifecycle, texture output, input forwarding, and license review |
| Implement the used GFx extension subset | 3 to 5 weeks | Focus, navigation, input adaptation, controls, text and mouse behaviors from the inventory above |
| Bridge vanilla GameDelegate operations | 6 to 10 weeks | Typed API groups for HUD and dialogs, journal and system, inventory and magic, then specialized menus |
| Run vanilla and mod compatibility surveys | 3 to 5 weeks | Installed vanilla menus, SkyUI, MCM and at least one menu-mod fixture per API family |
| Load-order runtime selection and diagnostics | 1 to 2 weeks | Preserve native Godot menus as fallback and report unsupported SWFs and APIs |

Proposed rows, after the prototype establishes a supported boundary:

1. **UI-27: Embed pinned Ruffle and present one interface SWF.** Prove runtime build, texture presentation, lifecycle, input forwarding, and a license and dependency review.
2. **UI-28: Implement the measured Scaleform extension subset.** Turn the source inventory into compatibility tests and add the required `gfx` APIs in dependency order.
3. **UI-29: Bridge vanilla menu GameDelegate calls by API family.** Start with menu families that share callbacks, then add specialized menus with installed-data tests.
4. **UI-30: Select and diagnose native or SWF menu execution by load order.** Test mod overrides, unsupported calls, fallback behavior, and the compatibility survey.

UI-23 does not change menu execution. It records the research boundary before
any runtime or host bridge is selected.

### Sources

- [SkyUI Community ActionScript source](https://github.com/doodlum/SkyUI-Community/tree/master/source/actionscript), supplied locally at `engine-refs/SkyUI-Community/source/actionscript`. The counts above are literal identifier and call-site scans of its 152 `.as` files. No source files were copied.
- [Ruffle](https://github.com/ruffle-rs/ruffle), a Rust Flash emulator, dual licensed MIT or Apache-2.0. Its project describes ActionScript 1 and 2 support.
- [Ruffle PR 9090](https://github.com/ruffle-rs/ruffle/pull/9090), merged change accepting `GFX` and `CFX` file signatures.
- UI-19 implementation in `crates/swf-convert/src/{cache,convert}.rs`, which converts SWF art and retains decompiled ActionScript outside the repository for reference only.
- Autodesk Scaleform [AS2 extensions reference](https://help.autodesk.com/cloudhelp/ENU/Scaleform-Help/scaleform_help/as2_reference.html), which documents the Scaleform-specific extension boundary.
