# ImGui menu framework research, UI-21

This note compares the public integration surfaces for SKSE Menu Framework and
dMenu with ways to host Dear ImGui inside the Godot runtime. It is research for
future rows. It does not implement either framework.

## Public APIs exposed to mods

### SKSE Menu Framework

SKSE Menu Framework gives an SKSE plugin a shared Mod Control Panel and a
callback to draw a page with Dear ImGui. Its SDK documents these entry points:

- `SetSection(name)` selects the plugin's group in the panel.
- `AddSectionItem(name, render_callback)` registers a page under that group.
- `AddWindow(render_callback, pause_game)` and `AddWindowWithView` register
  standalone windows.
- `GetMainWindow` returns the shared main window, whose state includes
  `IsOpen` and `BlockUserInput`.
- `AddInputEvent`, `AddHudElement` and `AddEvent` register callbacks. Their
  RAII handles unregister on destruction.
- `SetHotkeyEnabled`, `IsHotkeyEnabled`, `IsAnyBlockingWindowOpened`,
  `SetWindowsPauseGame`, `LoadTexture`, `DisposeTexture` and `PushFont` expose
  menu state and drawing resources.
- The header forwards Dear ImGui calls to the framework's exported functions.

The compatibility target is these public names and callback semantics, not
Skyrim executable hooks, Windows DLL loading or binary compatibility with an
SKSE plugin. Source-level adaptations can link against a matching shim API.
The copied SDK header at `engine-refs/SexLabpp/lib/ImGui/SKSEMenuFramework.h`
corroborates the function names and window state. The public SDK repository
also documents registering a section and page.

### dMenu

Original dMenu is a standalone ImGui cheat menu. Its published source does
not document a public API for other plugins. dMenu NG extends it with custom
settings pages backed by INI values. A settings entry describes controls and
the file/key they update. dMenu NG handles rendering and persistence. Its
optional frontend integrations expose those same setting pages through other
menu frameworks.

The current dMenu NG public header describes a separate, versioned external
control API:

- `dMenu_OpenMenu`, `dMenu_CloseMenu`, `dMenu_ToggleMenu` and
  `dMenu_SetMenuOpen(bool)` control the menu.
- `dMenu_IsMenuOpen`, `dMenu_IsReady`, `dMenu_GetApiVersion` and
  `dMenu_GetInterface(version)` expose state and API negotiation.

This narrow control surface is distinct from SKSE Menu Framework's immediate
mode drawing callbacks. UI-21 proposes compatibility for both shapes, rather
than treating dMenu settings definitions as arbitrary ImGui pages.

## What belongs in the MIT shim

The shim may define a source-compatible, project-owned API for the public
contracts: register and unregister a menu page or callback, set page/window
open state, query whether a blocking menu is open, request pause and receive
input/HUD/menu events. It may also declare dMenu's versioned open/close/query
interface. It must not contain Dear ImGui, rendering code, a port of either
framework, or GPL-derived implementation code.

The API should use engine-owned opaque handles and callbacks across the shim
boundary. The GPL core host owns callback scheduling, input routing, menu state,
and rendering. This keeps the MIT crate independent of GPL implementation
details while allowing its declarations to describe compatible behavior.

## ImGui inside the Godot host

Two implementation paths fit the host boundary:

| Path | Rendering and input | License and fit |
|---|---|---|
| `imgui-godot` by pkdawson | Godot 4 GDExtension draws ImGui in the scene tree and supplies input. It supports GDScript, C++ and C# entry points, with an `imgui_layout` signal for layout callbacks. | The plugin is MIT. Its native backend and bundled Dear ImGui version must stay matched. It is a ready-made Godot integration, but adds a separate native extension and versioned dependency to a Rust GDExtension host. |
| Rust `imgui` (`imgui-rs`) with a host-owned renderer | Rust produces ImGui draw data. A small adapter translates vertices, indices, clips and textures to Godot canvas or rendering-server resources, and forwards Godot input into the context. | `imgui-rs` is dual MIT or Apache-2.0, Dear ImGui is MIT, and Godot is MIT. The renderer and event bridge would be project code in the GPL core. It fits the Rust host and lets Godot own the window and render scheduling, but needs explicit font, texture, input capture, clipping and lifecycle work. |

Recommendation: prototype the Rust `imgui` path in `godot-host`. The host is
already Rust and owns Godot input and rendering. Keep the adapter private to
the GPL crate. Before adopting it, verify that the chosen `imgui` release's
Dear ImGui version, safe API, texture model and license metadata match the
renderer. Do not add a second Godot extension unless the prototype shows the
Rust adapter is impractical.

The renderer must use Godot resources and the existing host lifecycle. A menu
page callback runs during a host-owned ImGui frame. Its output is uploaded and
drawn in the UI layer, while Godot input is offered to ImGui before gameplay
input when a blocking window claims it. Pause requests and callback removal
must take effect when the framework handle is released. This keeps mod pages
as UI callbacks and does not introduce per-reference scene nodes.

## License findings

| Piece | License finding | Placement |
|---|---|---|
| `commonlib-shim` | MIT, fixed by this repository | Public declarations and ABI only. No GPL-derived code. |
| Thiago099 SKSE Menu Framework repository and SDK repository | MIT in those repositories | Read their public declarations and examples. Do not assume a separate runtime distribution has the SDK's license. |
| QTR-Modding SKSE Menu Framework 3 source | GPL-3.0 in the current upstream source license | Reference behavior only. Implement a clean API from public contracts, with no copied GPL implementation in the shim. |
| Original dMenu | MIT, as stated by its source repository | Public API and behavior reference. |
| dMenu NG implementation | GPL-3.0-or-later with its stated exceptions, because it statically links CommonLibSSE-NG | Reference GPL core behavior only. Its external API header has a separate MIT license. |
| `imgui-godot` | MIT | Optional candidate integration, not copied into the shim. |
| Dear ImGui | MIT | Can be used by GPL core host code with its notices retained. |
| Rust `imgui-rs` | MIT or Apache-2.0 | Candidate binding for GPL core host code. |
| Godot | MIT | Existing host platform. |

The terms above describe the projects and files checked for this research.
Review the exact release's license files and notices before taking a
dependency, porting code or distributing a binary. A permissive SDK or API
header license does not relicense the implementation it calls.

## Proposed rows

Each row stays within two crates and owns one deliverable. These are proposals
for the UI backlog, not claims of implementation.

| Proposed row | Crates | Deliverable | Depends on |
|---|---|---|---|
| UI-27 Define menu framework ABI | `commonlib-shim` | MIT declarations for page/window registration, event and input callback handles, open/block/pause state, and dMenu versioned control calls. Synthetic C++ contract tests cover registration and handle release. | UI-01 |
| UI-28 Render registered ImGui pages in Godot | `godot-host` | One host-owned ImGui context, frame lifecycle, renderer resources, input routing and pause behavior, with a Godot scene check. | UI-27 |
| UI-29 Execute SKSE Menu Framework style pages | `commonlib-shim`, `godot-host` | Exercise the API from an adapted plugin fixture and render a page, open/close its window and verify blocking input behavior. | UI-27, UI-28 |
| UI-30 Load dMenu NG custom settings definitions | `commonlib-shim`, `godot-host` | Parse the documented settings definition subset, render its controls through the shared menu API, and persist changed INI keys. Keep dMenu NG-specific APIs separately versioned. | UI-27, UI-28 |

## Sources read

- Local `engine-refs/SexLabpp/lib/ImGui/SKSEMenuFramework.h`, for callback
  names. The enclosing SexLabpp repository has an Apache-2.0 license. It does

... (+98 lines) [see remaining: rtk proxy git show ':2:docs/ui-frameworks.md' | tail -n +127]

## HTML menu framework research, UI-22

This note compares ways to host mod HTML menus and the public PrismaUI
contract. It is research for future rows. It does not add a browser renderer.

### Godot embedding choices

Godot core has no HTML or browser renderer for native game UI. Its web export
builds a Godot game to run in a browser, which is a different use case. Three
embedding choices are relevant:

| Choice | Rendering and deployment | License and size |
|---|---|---|
| CEF through [gdCEF](https://github.com/Lecrapouille/gdcef) | CEF off-screen rendering can update a Godot texture for a `TextureRect` or a 3D material. The browser and render subprocess ship with the game. | gdCEF is MIT. CEF has LGPL third-party libraries and must ship their notices and required materials. The checked CEF distribution is about 600 MB to download and about 1 GB of built artifacts. These are upstream artifact measurements, not a fixed minimum. |
| System webview through [Godot WRY](https://github.com/doceazedo/godot_wry) | Uses WebView2 on Windows, WebKit on macOS and WebKitGTK on Linux. It draws as a native view that always renders above Godot, so it cannot become a Godot texture for a 3D surface. | The addon is MIT and WRY is MIT or Apache-2.0. It uses the platform webview. No official runtime bundle size is published. |
| [Ultralight SDK](https://ultralig.ht/) | Embeds a GPU accelerated HTML renderer and can produce pixels for a Godot texture, including 3D surfaces. It ships its own runtime. | Ultralight's Free License Agreement defines its free-use terms. The SDK download is about 600 MB in the checked distribution and the agreement does not publish a fixed bundle size for a Godot integration. |

CEF's browser runtime adds a large download and deployment footprint, but gives
stable Chromium behavior and texture output. The system webview has a smaller
application bundle, but browser behavior varies by operating system and its
native overlay cannot be composed into the world. Ultralight also supports
texture output, though its agreement and bundled runtime need separate review.

### PrismaUI plugin API

PrismaUI exposes a versioned SKSE plugin interface. A plugin calls
`RequestPluginAPI(InterfaceVersion::V1)`, then creates one or more views from
HTML under `Data/PrismaUI/views/`. `CreateView` returns a view handle and a DOM ready callback. The plugin can show, hide, focus and destroy that view. It can
run JavaScript with `Invoke`, register JavaScript listeners for events flowing
back to native code, and unregister those listeners during teardown. The
compatibility surface is this view and event contract, including focus, visibility, ordering and destruction. A Godot-side compatible adapter does not load an SKSE DLL.

The API reference documents version negotiation, view construction and
`Invoke`. PrismaUI's samples also show native callbacks and JavaScript
listeners. A project-owned adapter still needs to define its supported event
payloads and lifecycle rules.

### Recommendation and proposed rows

Keep native Godot controls as the default. For a browser-backed mod menu,
prototype and use gdCEF's off-screen texture path behind an optional feature.
Measure memory, frame cost, input focus, transparency and texture upload with
representative PrismaUI views before making it a supported renderer. Do not adopt WRY for menus that need a Godot texture or world-space rendering.
Revisit Ultralight only if CEF's deployment size or performance is unacceptable
and its current license terms fit distribution.

| Proposed row | Deliverable |
|---|---|
| Specify the PrismaUI-compatible view and event contract | Define version negotiation, view lifecycle, callback order, focus, visibility, JavaScript calls and event payloads. |
| Add an optional CEF renderer adapter | Build gdCEF as an optional host integration and render browser output into a Godot texture. |
| Validate the adapter against named PrismaUI mods | Run representative plugins through their view and event flows and record compatibility gaps. |
| Survey HTML asset loading and script permissions | Define local asset resolution, network policy, script capabilities and error reporting. |

### Sources

- [Godot WRY](https://github.com/doceazedo/godot_wry), for platform webviews,
  overlay behavior and license.
- [gdCEF](https://github.com/Lecrapouille/gdcef), for CEF texture output,
  artifacts and the project's license statement.
- [PrismaUI API negotiation](https://prisma-user-interface-framework.github.io/Prisma2.0/docs/api/RequestPluginAPI/),
  [CreateView](https://prisma-user-interface-framework.github.io/Prisma2.0/docs/api/CreateView/),
  [Invoke](https://prisma-user-interface-framework.github.io/Prisma2.0/docs/api/Invoke/)
  and its JavaScript listener documentation, for the mod-facing API.
- [Ultralight license terms](https://ultralig.ht/terms/), for the Free License
  Agreement. Check the agreement for the exact release before distribution.
- Godot's stable [web export documentation](https://docs.godotengine.org/en/stable/tutorials/export/exporting_for_web.html),
  which describes exporting a Godot project to run in a browser rather than
  embedding a browser in a native project.

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
| Scaleform text and mouse extensions | `TextField.SetText` is absent from Ruffle AVM1; the source also uses MovieClip and mouse APIs | Implement `SetText` and test MovieClip and mouse behavior against the source menus |

This inventory is a source-level compatibility map, not a claim that stock
Ruffle lacks every similarly named Adobe Flash feature. The API surface was
checked against Ruffle nightly 2026-10-09 at commit `fbb4ce8`. Its AVM1 global
registration has no `gfx` namespace, so the listed `gfx.io`, `gfx.ui`,
`gfx.managers`, `gfx.controls` and `gfx.events` symbols are absent from stock
Ruffle. The same revision's AVM1 `TextField` implementation has no `SetText`
member, so `TextField.SetText` is a missing Scaleform extension. This check
does not mark standard Flash methods such as `MovieClip.gotoAndStop` as
missing. The pinned source links are [AVM1 globals](https://github.com/ruffle-rs/ruffle/blob/fbb4ce8/core/src/avm1/globals.rs)
and [AVM1 TextField](https://github.com/ruffle-rs/ruffle/blob/fbb4ce8/core/src/avm1/globals/text_field.rs).
Ruffle's project description says ActionScript 1 and 2 are supported, while
Scaleform's reference describes its extensions as Scaleform-only APIs.

### GameDelegate calls by vanilla source class

The table lists literal call and callback names in the supplied Vanilla and
PauseMenu ActionScript directories. Shared classes are included so the bridge
inventory covers all source classes. Names are case-sensitive.

| Source class | GameDelegate calls and callbacks |
|---|---|
| `PauseMenu/JournalBottomBar` | `PlaySound`, `RememberCurrentTabIndex`, `RequestPlayerInfo` |
| `PauseMenu/JournalSaveLoadList` | `PlaySound` |
| `PauseMenu/Quest_Journal` | `CloseMenu`, `HideMenu`, `PlaySound`, `RestoreSavedSettings`, `ShouldShowMod`, `ShowMenu`, `StartCloseMenu`, `onRightStickInput` |
| `PauseMenu/QuestsPage` | `PlaySound`, `RequestObjectivesData`, `RequestQuestsData`, `ShowTargetOnMap`, `ToggleQuestActiveStatus`, `ToggleShowMiscObjectives` |
| `PauseMenu/SettingsOptionItem` | `OptionChange`, `PlaySound` |
| `PauseMenu/StatsPage` | `PlaySound`, `updateStats` |
| `PauseMenu/SystemPage` | `BackOutFromLoadGame`, `CharacterSelected`, `FinishRemapMode`, `LOAD`, `LoadGame`, `ModManager`, `ORBISDeleteSave`, `OnSaveDataEventLoadCANCEL`, `OnSaveDataEventSaveCANCEL`, `OnSaveDataEventSaveSUCCESS`, `OpenKinectTuner`, `OptionChange`, `PlaySound`, `PopulateCharacterList`, `PopulateCreationClubTopics`, `PopulateHelpTopics`, `QuickSave`, `QuitToDesktop`, `QuitToMainMenu`, `RefreshSystemButtons`, `RequestAudioOptions`, `RequestCreationClubText`, `RequestDisplayOptions`, `RequestGameplayOptions`, `RequestHelpText`, `RequestInputMappings`, `RequestIsOnPC`, `ResetControlsToDefaults`, `SAVE`, `SaveControls`, `SaveGame`, `SaveSettings`, `SetRemoteDevice`, `SetSaveDisabled`, `SetVersionText`, `SettingsSaved`, `ShouldShowKinectTunerOption`, `StartRemapMode`, `UpdatePermissions`, `UseCurrentCharacterFilter` |
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

### Implementation estimates and proposed rows

These estimates cover one contributor familiar with the host and SWF formats.
They include a working prototype and its focused checks, but not a complete
vanilla menu port or a compatibility guarantee for third-party mods.

| Work item | Estimate |
|---|---|
| Pin Ruffle, build an embedded runtime, and render one static vanilla SWF | 1 to 2 weeks |
| Implement the used GFx extension subset | 3 to 5 weeks |
| Bridge vanilla GameDelegate operations | 6 to 10 weeks |
| Run vanilla and mod compatibility surveys | 3 to 5 weeks |
| Load-order runtime selection and diagnostics | 1 to 2 weeks |

**UI-27: Pin Ruffle and render one vanilla SWF.** Record the pinned revision,
build path, Godot texture output and resource cost.

**UI-28: Implement the used Scaleform extensions.** Add the GFx APIs found
in the source survey with tests for their menu-visible behavior.

**UI-29: Bridge vanilla GameDelegate operations.** Implement typed handlers
for the calls listed by source class, preserving callback and argument order.

**UI-30: Survey compatibility and runtime selection.** Exercise vanilla and
named mod menus, report unsupported operations, and define load-order selection
and diagnostics.

### Sources and verification limits

- The local `SkyUI-Community/source/actionscript` tree, for the file, call,
  callback and namespace inventory.
- [Ruffle](https://github.com/ruffle-rs/ruffle), for its project description
  and ActionScript 1 and 2 support statement.
- Ruffle nightly 2026-10-09, commit `fbb4ce8`, for the pinned AVM1 global and
  TextField surface audit.
- [Ruffle pull request 9090](https://github.com/ruffle-rs/ruffle/pull/9090),
  for the `GFX` and `CFX` header signature change.
- The research lead at `research/engine-map/leads/ui-scaleform.md` and the
  local Ruffle clone were absent from this checkout. The pinned upstream source
  links above were checked directly for the AVM1 namespace and `TextField.SetText`
  findings. Compatibility behavior still needs a runtime prototype and menu
  tests.
- Scaleform extension behavior is described as a compatibility target from
  source usage. Verify each method against primary references during UI-28.
