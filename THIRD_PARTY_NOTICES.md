# Third-party notices

## Mudcrab

Source: <https://github.com/Mudcrab-Team/mudcrab>. Ported into the GPL-3.0-or-later
core crates under the MIT License below.

- Character controller in `crates/physics/src/controller.rs` follows
  `crates/engine/src/physics.rs` at `268f988` and PR 232.
- CPU mesh attribute, index and image payload accounting in
  `crates/godot-host/src/memory.rs` is adapted from
  `crates/engine/src/profiling.rs`, PR 224.
- Streaming lifecycle in `crates/godot-host/src/streaming.rs` follows
  `crates/engine/src/streaming.rs` at `268f9884f7acaaa0302425331064f7268aac9d3c`.
- Demand ordering and tests in `crates/godot-host/src/streaming/priority.rs`
  port `crates/engine/src/streaming/priority.rs` from PR 210 at
  `4db055d4b99411e4b313557240efd7f4f482984d`.
- Drawable handoff in `crates/godot-host/src/streaming.rs` follows
  `docs/research/streaming-priority.md` from PR 210.
- Frame readiness counters in `crates/trace/src/stream.rs` follow
  `docs/research/streaming-trace.md` from PR 206 at
  `1fd08d70b945a1eab76a5259fc0cffefbff343da`.
- Matched route observations in `crates/godot-host/tests/mudcrab_streaming_route`
  follow `docs/research/streaming-comparison-results.md` from PR 212 at
  `97be55d3cd9302b648854529e92ee43d384ae53c`.

```text
MIT License

Copyright (c) 2026 Mudcrab Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
