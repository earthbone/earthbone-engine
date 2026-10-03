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
