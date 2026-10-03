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
the file/key they update; dMenu NG handles rendering and persistence. Its
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
  names. The enclosing SexLabpp repository has an Apache-2.0 license; it does
  not establish a separate license for this copied header.
- [SKSE Menu Framework SDK](https://github.com/Thiago099/SKSE-Menu-Framework-SDK)
  and [SKSE Menu Framework source](https://github.com/Thiago099/SKSE-Menu-Framework),
  for page registration and MIT repository licenses.
- [SKSE Menu Framework 3 source](https://github.com/QTR-Modding/SKSE-Menu-Framework-3)
  and its [Nexus page](https://www.nexusmods.com/skyrimspecialedition/mods/120352),
  for current API behavior and the runtime source license.
- [Original dMenu source](https://github.com/D7ry/dMenu), for project identity
  and its MIT license.
- [dMenu NG source](https://github.com/c0kadam/dmenu-NG), for the public
  versioned control API and separation between its API header and GPL runtime.
- [imgui-godot](https://github.com/pkdawson/imgui-godot), including its
  README and wiki, for the GDExtension, callback and input integration model.
- [imgui-rs](https://github.com/imgui-rs/imgui-rs), for Rust bindings, draw
  data ownership and dual MIT or Apache-2.0 license.
- The existing [shim](shim.md), [Godot host](godot-host.md) and
  [style](style.md) docs, for crate boundaries and project rules.

The requested framework repositories were not present as working clones in
`engine-refs`; the local SDK header was available. The linked upstream source
pages supplied API and license details where the local corpus had no copy.
# HTML menus and PrismaUI compatibility, UI-22

Godot core has no HTML or browser renderer. HTML menus therefore need an
embedded web renderer and a bridge to the engine's menu and input lifecycle.
This matters for compatibility with mods written for PrismaUI. The engine's
default menus remain native Godot controls, following the SkyUI layout and
behaviour in [intent.md](intent.md).

## Embedding choices

| Renderer | Licence and bundle size | Godot texture output | Integration notes |
|---|---|---|---|
| CEF through [gdCEF](https://github.com/Lecrapouille/gdcef) | gdCEF is MIT. CEF has LGPL third-party libraries. Its installation guide reports about 600 MB to download and about 1 GB of built artifacts. | Yes. gdCEF creates a browser attached to a `TextureRect`, and includes 2D and 3D examples. | Bundles Chromium, subprocesses, libraries and resources. It needs per-platform artifacts. |
| System webview through [Godot WRY](https://github.com/doceazedo/godot_wry) | The addon is MIT and WRY is MIT or Apache-2.0. The addon does not publish a fixed bundle size because it uses the platform webview. Linux needs WebKitGTK installed. | No. The webview is an operating-system surface that always renders above Godot, not a `Texture2D` for 2D or 3D materials. | Small addon footprint, but different browser engines across Windows, macOS and Linux. Godot WRY currently documents Linux X11 support. |
| Ultralight SDK | Ultralight's Free License Agreement governs the SDK. The free grant is not a general redistribution grant. Commercial distribution requires the agreement's distribution license, and its terms include revenue and funding limits. PrismaUI uses the free SDK and has its own source-available license. No official runtime bundle size is published in the reviewed pages. | Yes, with renderer integration. Ultralight exposes GPU and CPU renderers and GPU integration APIs. A Godot adapter still has to upload or share the rendered surface as a Godot texture. | WebKit-based and substantially lighter in baseline memory than Chromium according to the vendor. PrismaUI itself integrates Ultralight with Skyrim's DirectX 11 renderer, so its rendering code cannot be reused as a Godot renderer. |

The stated CEF download and artifact sizes come from gdCEF's build guide and
are estimates, not measurements of a packaged engine build. WRY's platform
webview is supplied by the operating system, so its browser update and bundle
size are outside the addon. The reviewed Ultralight materials make no numerical
runtime-size claim. Recheck the current [Ultralight terms](https://ultralig.ht/terms/ultralight-subscription-agreement-v2.pdf)
before redistributing its SDK or basing a public release on it.

## PrismaUI's mod API

PrismaUI is an SKSE plugin framework. A mod requests its versioned C++
interface with `RequestPluginAPI(InterfaceVersion::V1)`, then creates one
`PrismaView` for that plugin from an HTML path under
`Data/PrismaUI/views/`. The plugin can call `Invoke` to call JavaScript,
register JavaScript listeners to receive UI events, and use view methods for
focus, visibility, ordering and destruction. The interface supports passing
game data to the page and receiving commands back. PrismaUI uses HTML, CSS and
JavaScript, with JSON exchange in its public description and examples.

The native engine does not load an SKSE DLL or PrismaUI's Skyrim-specific
DirectX 11 renderer. Compatibility therefore means offering a corresponding
view lifecycle and bidirectional event/data bridge to HTML mods. It does not
mean loading the existing Skyrim binary plugin. HTML assets should be local to
the mod, and engine commands must pass through the shared Game APIs.

PrismaUI's framework has a custom source-available license. Its repository
allows use and redistribution of official files under listed conditions, but
does not allow public redistribution of a modified framework without written
permission. The included Ultralight SDK has separate license terms. These
terms make PrismaUI useful as an API reference, not as a component to vendor
or modify for this engine.

## Recommendation and proposed rows

Keep native Godot controls as the default. Build a PrismaUI-compatible host
contract around views, focus, visibility, ordering, destruction, JavaScript
calls and event delivery. For the first renderer prototype, use gdCEF's
offscreen texture path behind an optional feature so the team can test real
HTML mods without imposing its roughly 1 GB artifacts on every installation.
Measure memory, frame cost, input focus, transparency and texture upload before
making it a supported runtime option. Do not adopt WRY for in-scene menus
because its operating-system surface cannot be composited into Godot's 2D or
3D scene. Revisit Ultralight only after its SDK redistribution terms and
Godot texture integration are settled.

Proposed follow-up rows:

1. Specify the PrismaUI-compatible view and event contract in `godot-host`,
   including focus, lifecycle, typed commands into Game and version handling.
2. Add an optional CEF renderer adapter that displays one HTML view as a Godot
   texture and reports its measured package size and runtime cost.
3. Validate the adapter against named PrismaUI mods for mouse, keyboard,
   controller, pause state, focus handoff and close events.
4. Survey HTML asset loading and script permissions, then define how menus
   remain local and cannot bypass the shared Game API.

## Sources

- Local search of `/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs` found no PrismaUI, gdCEF or Godot WRY source clone.
- [PrismaUI Nexus page](https://www.nexusmods.com/skyrimspecialedition/mods/148718), [PrismaUI API](https://www.prismaui.dev/api/create-view/) and [PrismaUI framework license](https://github.com/PrismaUI-SKSE/framework/blob/main/LICENSE.md).
- [Ultralight Free License Agreement](https://ultralig.ht/terms/ultralight-subscription-agreement-v2.pdf) and [Ultralight game integration guide](https://docs.ultralig.ht/docs/game-integration).
- [Godot WRY repository](https://github.com/doceazedo/godot_wry) and [gdCEF repository](https://github.com/Lecrapouille/gdcef), including gdCEF's [installation guide](https://github.com/Lecrapouille/gdcef/blob/godot-4.x/doc/installation.md).
