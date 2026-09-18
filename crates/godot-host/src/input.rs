// SPDX-License-Identifier: GPL-3.0-or-later

//! Input, from the vanilla `controlmap.txt` to a frame of movement intent.
//!
//! No engine dependency here. The Godot layer reads devices and fills an
//! [`InputState`], this module turns that into an [`Intent`] the character
//! controller can be driven with, and nothing about Godot reaches the rules.
//!
//! ## The binding file is Bethesda's, not ours
//!
//! `interface/controlmap.txt` ships inside `Skyrim - Interface.bsa` and
//! documents itself in its own header: one user event per line, then the
//! keyboard, mouse and gamepad ids that fire it, three remappable flags and
//! an optional grouping flag. Blank lines start a new input context. Reading
//! that format means a mod that ships a `controlmap.txt` rebinds this engine
//! the way it rebinds the real one, which is the whole point of the project.
//!
//! Two things the shipped file settles, both measured rather than assumed
//! (`tests/controlmap.rs`):
//!
//! - The keyboard column holds **Windows virtual key codes**, not DirectInput
//!   scan codes. `F1` is `0x70`, `Enter` `0x0d`, the arrows `0x25` to `0x28`.
//! - It carries **no keyboard movement at all**. There is no `Forward` or
//!   `Strafe Left` event in the file, and `Activate`, `Jump`, `Sneak` and
//!   `Toggle POV` all sit at `0xff`, unmapped, with only a gamepad id. The
//!   retail engine holds the keyboard defaults internally and the file is the
//!   remapping surface over them.
//!
//! So [`Bindings::vanilla_keyboard`] is this engine's table, not Bethesda's,
//! for the same reason the camera offsets are. It matches the layout players
//! expect, the file overrides it wherever the file says anything, and a user
//! editing `controlmap.txt` gets the last word. Do not read those constants
//! as recovered data.

use std::collections::BTreeMap;

use soe_physics::{Vec3, FAST_WALK_SPEED, JOG_SPEED, RUN_SPEED, WALK_SPEED};

use crate::camera::CameraRig;

/// The id a control map uses for "this device does not fire this event".
pub const UNMAPPED: u32 = 0xff;

/// Windows virtual key codes for the keys the default layout uses. Named so
/// the table below reads as a keyboard rather than as magic numbers.
pub mod vk {
    pub const BACK: u32 = 0x08;
    pub const TAB: u32 = 0x09;
    pub const RETURN: u32 = 0x0d;
    pub const SHIFT: u32 = 0x10;
    pub const CONTROL: u32 = 0x11;
    pub const ALT: u32 = 0x12;
    pub const ESCAPE: u32 = 0x1b;
    pub const SPACE: u32 = 0x20;
    pub const A: u32 = 0x41;
    pub const C: u32 = 0x43;
    pub const D: u32 = 0x44;
    pub const E: u32 = 0x45;
    pub const F: u32 = 0x46;
    pub const M: u32 = 0x4d;
    pub const R: u32 = 0x52;
    pub const S: u32 = 0x53;
    pub const T: u32 = 0x54;
    pub const W: u32 = 0x57;
    pub const GRAVE: u32 = 0xc0;
}

/// Godot's `Key` values for the keys that are not plain ASCII.
///
/// Godot numbers printable keys by their ASCII codepoint and everything else
/// from `1 << 22`. Windows numbers its virtual keys differently above the
/// letters, so the two tables agree on `A` to `Z`, `0` to `9` and space, and
/// disagree on every modifier and named key. Values read from
/// `engine-refs/godot-docs/classes/class_@globalscope.rst`.
pub mod godot_key {
    pub const SPECIAL: i64 = 1 << 22;
    pub const ESCAPE: i64 = SPECIAL + 1;
    pub const TAB: i64 = SPECIAL + 2;
    pub const BACKSPACE: i64 = SPECIAL + 4;
    pub const ENTER: i64 = SPECIAL + 5;
    pub const LEFT: i64 = SPECIAL + 15;
    pub const UP: i64 = SPECIAL + 16;
    pub const RIGHT: i64 = SPECIAL + 17;
    pub const DOWN: i64 = SPECIAL + 18;
    pub const PAGEUP: i64 = SPECIAL + 19;
    pub const PAGEDOWN: i64 = SPECIAL + 20;
    pub const SHIFT: i64 = SPECIAL + 21;
    pub const CTRL: i64 = SPECIAL + 22;
    pub const ALT: i64 = SPECIAL + 24;
    /// The backtick and tilde key, which Windows calls `VK_OEM_3`.
    pub const QUOTELEFT: i64 = 96;
    /// `F1`, with `F2` to `F12` following one after another.
    pub const F1: i64 = SPECIAL + 28;
}

/// Translate a control map's Windows virtual key code into Godot's `Key`.
///
/// `None` for a code with no Godot equivalent, which the caller should treat
/// as an unbindable entry rather than substituting something.
#[must_use]
pub fn godot_key_for(vk: u32) -> Option<i64> {
    use godot_key as g;
    // Letters, digits and space share the ASCII codepoint in both tables.
    let shared = matches!(vk, 0x30..=0x39 | 0x41..=0x5a | 0x20);
    if shared {
        return Some(i64::from(vk));
    }
    Some(match vk {
        vk::BACK => g::BACKSPACE,
        vk::TAB => g::TAB,
        vk::RETURN => g::ENTER,
        vk::SHIFT => g::SHIFT,
        vk::CONTROL => g::CTRL,
        vk::ALT => g::ALT,
        vk::ESCAPE => g::ESCAPE,
        vk::GRAVE => g::QUOTELEFT,
        0x25 => g::LEFT,
        0x26 => g::UP,
        0x27 => g::RIGHT,
        0x28 => g::DOWN,
        0x21 => g::PAGEUP,
        0x22 => g::PAGEDOWN,
        // Left and right modifiers collapse onto the plain modifier, since
        // Godot's `Key` has no separate side and the control map uses the
        // sided codes only to offer an alternative chord.
        0xa0 | 0xa1 => g::SHIFT,
        0xa2 | 0xa3 => g::CTRL,
        0xa4 | 0xa5 => g::ALT,
        // F1 to F12 are contiguous in both tables, from different bases.
        0x70..=0x7b => g::F1 + i64::from(vk - 0x70),
        _ => return None,
    })
}

/// Several keys that must be held together, as `0x10+0x09` in the file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Chord(pub Vec<u32>);

impl Chord {
    #[must_use]
    pub fn single(key: u32) -> Self {
        Chord(vec![key])
    }

    /// Whether every key in the chord is currently down.
    #[must_use]
    pub fn held(&self, is_down: &dyn Fn(u32) -> bool) -> bool {
        !self.0.is_empty() && self.0.iter().all(|k| is_down(*k))
    }
}

/// What one device offers for one event: any of these chords fires it, as
/// `0x10+0x09,0xa1+0x09` in the file. Empty means unmapped.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Binding(pub Vec<Chord>);

impl Binding {
    #[must_use]
    pub fn unmapped() -> Self {
        Binding(Vec::new())
    }

    #[must_use]
    pub fn is_unmapped(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn key(key: u32) -> Self {
        Binding(vec![Chord::single(key)])
    }

    #[must_use]
    pub fn held(&self, is_down: &dyn Fn(u32) -> bool) -> bool {
        self.0.iter().any(|c| c.held(is_down))
    }

    /// Parse one device column. `0xff` and an empty field are unmapped.
    ///
    /// The file writes alternatives with a comma and a chord with a plus, and
    /// every number is hexadecimal with an `0x` prefix.
    #[must_use]
    pub fn parse(field: &str) -> Self {
        let field = field.trim();
        if field.is_empty() {
            return Binding::unmapped();
        }
        let mut out = Vec::new();
        for alt in field.split(',') {
            let mut keys = Vec::new();
            let mut ok = true;
            for part in alt.split('+') {
                match parse_id(part.trim()) {
                    Some(id) if id != UNMAPPED => keys.push(id),
                    // An `0xff` anywhere means this alternative is unmapped,
                    // and an unparsable field is treated the same way rather
                    // than guessed at.
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok && !keys.is_empty() {
                out.push(Chord(keys));
            }
        }
        Binding(out)
    }
}

fn parse_id(s: &str) -> Option<u32> {
    let s = s.trim();
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"));
    match hex {
        Some(h) => u32::from_str_radix(h, 16).ok(),
        None => s.parse::<u32>().ok(),
    }
}

/// One line of a control map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapping {
    /// The user event name, which the file's own header says never to alter.
    pub event: String,
    pub keyboard: Binding,
    pub mouse: Binding,
    pub gamepad: Binding,
    pub remap_keyboard: bool,
    pub remap_mouse: bool,
    pub remap_gamepad: bool,
    /// The grouping flag, absent on a few lines.
    pub flags: Option<u32>,
}

/// One input context, the blocks the file separates with blank lines. The
/// name is the `//` comment that introduced it, when there was one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Context {
    pub name: Option<String>,
    pub mappings: Vec<Mapping>,
}

impl Context {
    #[must_use]
    pub fn find(&self, event: &str) -> Option<&Mapping> {
        self.mappings.iter().find(|m| m.event == event)
    }
}

/// A parsed `controlmap.txt`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ControlMap {
    pub contexts: Vec<Context>,
}

impl ControlMap {
    /// Parse the whole file. Unreadable lines are skipped rather than fatal,
    /// since a mod's control map is user data and one bad line should not
    /// cost the player every binding in the file.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut contexts = Vec::new();
        let mut current = Context::default();
        let mut pending_name: Option<String> = None;

        for raw in text.lines() {
            let line = raw.trim_end();
            let trimmed = line.trim();

            if trimmed.is_empty() {
                if !current.mappings.is_empty() {
                    contexts.push(std::mem::take(&mut current));
                }
                current.name = pending_name.take();
                continue;
            }
            if let Some(comment) = trimmed.strip_prefix("//") {
                let text = comment.trim();
                // A short comment right before a block names it. The long
                // header paragraphs are not names, so anything with a
                // sentence in it is ignored.
                if !text.is_empty() && text.len() < 40 && !text.contains('.') {
                    if current.mappings.is_empty() {
                        current.name = Some(text.to_string());
                    } else {
                        pending_name = Some(text.to_string());
                    }
                }
                continue;
            }

            if let Some(m) = parse_mapping(line) {
                current.mappings.push(m);
            }
        }
        if !current.mappings.is_empty() {
            contexts.push(current);
        }
        ControlMap { contexts }
    }

    /// The first context, which is Main Gameplay in the shipped file.
    #[must_use]
    pub fn gameplay(&self) -> Option<&Context> {
        self.contexts.first()
    }

    /// Look an event up across every context, first match winning. Useful
    /// because the gameplay events this engine drives are unique by name.
    #[must_use]
    pub fn find(&self, event: &str) -> Option<&Mapping> {
        self.contexts.iter().find_map(|c| c.find(event))
    }

    #[must_use]
    pub fn mapping_count(&self) -> usize {
        self.contexts.iter().map(|c| c.mappings.len()).sum()
    }
}

fn parse_mapping(line: &str) -> Option<Mapping> {
    // Fields are tab separated and the run of tabs is decorative, so empty
    // splits are dropped. The event name keeps its spaces, which is why the
    // line cannot be split on whitespace: "Left Attack/Block" is one field.
    let fields: Vec<&str> = line
        .split('\t')
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .collect();
    if fields.len() < 7 {
        return None;
    }
    let event = fields[0].to_string();
    if event.starts_with("//") {
        return None;
    }
    let flag = |s: &str| s == "1";
    Some(Mapping {
        event,
        keyboard: Binding::parse(fields[1]),
        mouse: Binding::parse(fields[2]),
        gamepad: Binding::parse(fields[3]),
        remap_keyboard: flag(fields[4]),
        remap_mouse: flag(fields[5]),
        remap_gamepad: flag(fields[6]),
        flags: fields.get(7).and_then(|f| parse_id(f)),
    })
}

/// The gameplay events this engine acts on. The strings are the vanilla event
/// names where one exists, so a control map naming them binds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Action {
    Forward,
    Back,
    StrafeLeft,
    StrafeRight,
    Jump,
    Sneak,
    Sprint,
    /// Hold to walk instead of jog, the inverse of the always-run toggle.
    Walk,
    ToggleAlwaysRun,
    Activate,
    TogglePov,
    ReadyWeapon,
}

impl Action {
    /// The vanilla user event name, where the file has one. `Forward` and the
    /// other three movement directions have no vanilla event, so they carry
    /// the names the retail remapping UI shows.
    #[must_use]
    pub fn event_name(self) -> &'static str {
        match self {
            Action::Forward => "Forward",
            Action::Back => "Back",
            Action::StrafeLeft => "Strafe Left",
            Action::StrafeRight => "Strafe Right",
            Action::Jump => "Jump",
            Action::Sneak => "Sneak",
            Action::Sprint => "Sprint",
            Action::Walk => "Walk",
            Action::ToggleAlwaysRun => "Toggle Always Run",
            Action::Activate => "Activate",
            Action::TogglePov => "Toggle POV",
            Action::ReadyWeapon => "Ready Weapon",
        }
    }

    #[must_use]
    pub fn all() -> [Action; 12] {
        [
            Action::Forward,
            Action::Back,
            Action::StrafeLeft,
            Action::StrafeRight,
            Action::Jump,
            Action::Sneak,
            Action::Sprint,
            Action::Walk,
            Action::ToggleAlwaysRun,
            Action::Activate,
            Action::TogglePov,
            Action::ReadyWeapon,
        ]
    }
}

/// What each action is bound to on the keyboard, after the control map has
/// had its say over the defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bindings {
    keyboard: BTreeMap<Action, Binding>,
}

impl Default for Bindings {
    fn default() -> Self {
        Self::vanilla_keyboard()
    }
}

impl Bindings {
    /// This engine's keyboard layout, matching what a Skyrim player expects.
    /// Not recovered from the game, see the module note.
    #[must_use]
    pub fn vanilla_keyboard() -> Self {
        let pairs = [
            (Action::Forward, vk::W),
            (Action::Back, vk::S),
            (Action::StrafeLeft, vk::A),
            (Action::StrafeRight, vk::D),
            (Action::Jump, vk::SPACE),
            (Action::Sneak, vk::CONTROL),
            (Action::Sprint, vk::ALT),
            (Action::Walk, vk::SHIFT),
            (Action::ToggleAlwaysRun, vk::C),
            (Action::Activate, vk::E),
            (Action::TogglePov, vk::F),
            (Action::ReadyWeapon, vk::R),
        ];
        Bindings {
            keyboard: pairs
                .into_iter()
                .map(|(a, k)| (a, Binding::key(k)))
                .collect(),
        }
    }

    /// Override any action the control map binds on the keyboard. An event
    /// the file leaves at `0xff` keeps the default, which is what makes the
    /// shipped file usable: it unmaps almost every gameplay key.
    pub fn apply_control_map(&mut self, map: &ControlMap) {
        for action in Action::all() {
            if let Some(m) = map.find(action.event_name()) {
                if !m.keyboard.is_unmapped() {
                    self.keyboard.insert(action, m.keyboard.clone());
                }
            }
        }
    }

    #[must_use]
    pub fn get(&self, action: Action) -> Option<&Binding> {
        self.keyboard.get(&action)
    }

    /// Whether the action's keys are down under the caller's key probe.
    #[must_use]
    pub fn held(&self, action: Action, is_down: &dyn Fn(u32) -> bool) -> bool {
        self.keyboard.get(&action).is_some_and(|b| b.held(is_down))
    }
}

/// One frame of raw intent, filled by whoever is reading the devices.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct InputState {
    /// Strafe, positive right.
    pub move_x: f32,
    /// Forward, positive ahead.
    pub move_y: f32,
    /// Mouse motion this frame, in device units.
    pub look_dx: f32,
    pub look_dy: f32,
    pub sprint: bool,
    pub walk: bool,
    pub sneak_held: bool,
    /// Edge triggered, true only on the frame the key went down.
    pub jump_pressed: bool,
    pub sneak_pressed: bool,
    pub activate_pressed: bool,
    pub toggle_pov_pressed: bool,
    pub toggle_always_run_pressed: bool,
}

impl InputState {
    /// Read every action from a key probe and a mouse delta. The edge
    /// triggered fields need the previous frame, so `was_down` answers for
    /// the frame before.
    #[must_use]
    pub fn gather(
        bindings: &Bindings,
        is_down: &dyn Fn(u32) -> bool,
        was_down: &dyn Fn(u32) -> bool,
        mouse_right: f32,
        mouse_down: f32,
    ) -> Self {
        let now = |a: Action| bindings.held(a, is_down);
        let before = |a: Action| bindings.held(a, was_down);
        let pressed = |a: Action| now(a) && !before(a);

        let axis = |neg: Action, pos: Action| {
            f32::from(u8::from(now(pos))) - f32::from(u8::from(now(neg)))
        };

        InputState {
            move_x: axis(Action::StrafeLeft, Action::StrafeRight),
            move_y: axis(Action::Back, Action::Forward),
            look_dx: mouse_right,
            look_dy: mouse_down,
            sprint: now(Action::Sprint),
            walk: now(Action::Walk),
            sneak_held: now(Action::Sneak),
            jump_pressed: pressed(Action::Jump),
            sneak_pressed: pressed(Action::Sneak),
            activate_pressed: pressed(Action::Activate),
            toggle_pov_pressed: pressed(Action::TogglePov),
            toggle_always_run_pressed: pressed(Action::ToggleAlwaysRun),
        }
    }

    #[must_use]
    pub fn is_moving(&self) -> bool {
        self.move_x.abs() > 1e-6 || self.move_y.abs() > 1e-6
    }
}

/// What the character controller should be told this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Intent {
    /// Flat, normalised, in Skyrim units. Zero means stand still.
    pub direction: Vec3,
    /// One of the game's movement speeds, in units per second.
    pub speed: f32,
    pub jump: bool,
    /// The crouch state that should hold now, not an edge.
    pub crouch: bool,
    pub activate: bool,
}

/// Turns held keys into a speed and a direction. Holds the toggles, since
/// always-run and sneak are state rather than momentary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Driver {
    /// Vanilla ships this on, so a fresh player jogs rather than walks.
    pub always_run: bool,
    pub sneaking: bool,
}

impl Driver {
    #[must_use]
    pub fn new() -> Self {
        Driver {
            always_run: true,
            sneaking: false,
        }
    }

    /// Fold this frame's input into the held state and say what the
    /// controller should do. The camera supplies the facing, so movement is
    /// relative to the view exactly as it is in the real game.
    pub fn update(&mut self, input: &InputState, camera: &CameraRig) -> Intent {
        if input.toggle_always_run_pressed {
            self.always_run = !self.always_run;
        }
        if input.sneak_pressed {
            self.sneaking = !self.sneaking;
        }

        let direction = camera.move_direction(input.move_x, input.move_y);
        let moving = input.is_moving();

        // Sneaking is slowest and beats everything. Sprint beats the run and
        // walk pair. The walk key inverts whatever always-run is set to, so
        // holding it while always-run is on gives a walk and holding it while
        // always-run is off gives a jog.
        let speed = if !moving {
            0.0
        } else if self.sneaking {
            WALK_SPEED
        } else if input.sprint {
            RUN_SPEED
        } else if self.always_run != input.walk {
            JOG_SPEED
        } else {
            FAST_WALK_SPEED
        };

        Intent {
            direction,
            speed,
            // A sneaking character cannot jump, which is vanilla.
            jump: input.jump_pressed && !self.sneaking,
            crouch: self.sneaking,
            activate: input.activate_pressed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIPPED_SAMPLE: &str = "\
// 1st field: User event name.  DO NOT ALTER!  This field is used to ID events in the code
//
// Main Gameplay
Move\t\t\t\t0xff\t0xff\t0x000b\t\t\t0\t0\t0\t0x801
Left Attack/Block\t0xff\t0xff\t0x0009\t\t\t0\t0\t1\t0x841
Activate\t\t\t0xff\t0xff\t0x1000\t\t\t0\t0\t1\t0x804
Toggle POV\t\t\t0xff\t0xff\t0x0080\t\t\t0\t0\t1\t0x820
Console\t\t\t\t0xc0\t0xff\t0xff\t\t\t0\t0\t0\t0x10
CameraPath\t\t\t0x7b\t0xff\t0xff\t\t\t0\t0\t0
KinectShout\t\t\t0xff\t0xff\t0x0100+0x0200\t0\t0\t0\t0x840

// Menu Mode
Accept\t\t0xff\t0xff\t0x1000\t0\t0\t0\t0x8
PreviousFocus\t0x10+0x09,0xa1+0x09\t0xff\t0x0100\t0\t0\t0\t0x10
";

    fn map() -> ControlMap {
        ControlMap::parse(SHIPPED_SAMPLE)
    }

    #[test]
    fn an_event_name_keeps_its_spaces() {
        let m = map();
        assert!(m.find("Left Attack/Block").is_some());
        assert!(m.find("Toggle POV").is_some());
    }

    #[test]
    fn a_blank_line_starts_a_new_context() {
        let m = map();
        assert_eq!(m.contexts.len(), 2, "{:?}", m.contexts);
        assert_eq!(m.contexts[0].name.as_deref(), Some("Main Gameplay"));
        assert_eq!(m.contexts[1].name.as_deref(), Some("Menu Mode"));
    }

    #[test]
    fn the_long_header_paragraph_is_not_taken_as_a_context_name() {
        let m = map();
        // The first context is named by the short comment, not the sentence
        // above it.
        assert_eq!(m.contexts[0].name.as_deref(), Some("Main Gameplay"));
    }

    #[test]
    fn unmapped_columns_parse_as_unmapped() {
        let m = map();
        let activate = m.find("Activate").unwrap();
        assert!(activate.keyboard.is_unmapped());
        assert!(activate.mouse.is_unmapped());
        assert!(!activate.gamepad.is_unmapped());
    }

    #[test]
    fn a_keyboard_id_is_read_as_hex() {
        let m = map();
        let console = m.find("Console").unwrap();
        assert_eq!(console.keyboard, Binding::key(0xc0));
    }

    #[test]
    fn a_missing_flag_field_is_allowed() {
        let m = map();
        let path = m.find("CameraPath").unwrap();
        assert_eq!(path.flags, None);
        assert_eq!(path.keyboard, Binding::key(0x7b));
    }

    #[test]
    fn a_plus_is_a_chord_and_a_comma_is_an_alternative() {
        let m = map();
        let prev = m.find("PreviousFocus").unwrap();
        assert_eq!(
            prev.keyboard,
            Binding(vec![Chord(vec![0x10, 0x09]), Chord(vec![0xa1, 0x09])])
        );
        let shout = m.find("KinectShout").unwrap();
        assert_eq!(shout.gamepad, Binding(vec![Chord(vec![0x100, 0x200])]));
    }

    #[test]
    fn the_remappable_flags_are_read() {
        let m = map();
        let attack = m.find("Left Attack/Block").unwrap();
        assert!(!attack.remap_keyboard);
        assert!(!attack.remap_mouse);
        assert!(attack.remap_gamepad);
    }

    #[test]
    fn a_chord_needs_every_key() {
        let chord = Chord(vec![0x10, 0x09]);
        assert!(chord.held(&|k| k == 0x10 || k == 0x09));
        assert!(!chord.held(&|k| k == 0x10));
        assert!(!chord.held(&|_| false));
    }

    #[test]
    fn an_alternative_needs_only_one_chord() {
        let b = Binding(vec![Chord(vec![0x10, 0x09]), Chord(vec![0xa1, 0x09])]);
        assert!(b.held(&|k| k == 0xa1 || k == 0x09));
    }

    #[test]
    fn an_empty_binding_is_never_held() {
        assert!(!Binding::unmapped().held(&|_| true));
    }

    #[test]
    fn a_junk_line_is_skipped_not_fatal() {
        let m = ControlMap::parse("nonsense\nAlso nonsense\n\nActivate\t0x45\t0xff\t0xff\t1\t1\t1");
        assert_eq!(m.mapping_count(), 1);
        assert_eq!(m.find("Activate").unwrap().keyboard, Binding::key(0x45));
    }

    #[test]
    fn an_unparsable_id_is_unmapped_rather_than_guessed() {
        assert!(Binding::parse("wat").is_unmapped());
        assert!(Binding::parse("").is_unmapped());
        assert!(Binding::parse("0xff").is_unmapped());
    }

    #[test]
    fn the_defaults_cover_every_action() {
        let b = Bindings::default();
        for a in Action::all() {
            assert!(b.get(a).is_some(), "{a:?} has no default binding");
        }
    }

    #[test]
    fn the_shipped_map_does_not_disturb_the_keyboard_defaults() {
        // Every gameplay key in the shipped file is 0xff, so applying it must
        // leave the defaults alone. This is the behaviour that makes the real
        // file usable at all.
        let mut b = Bindings::default();
        let before = b.clone();
        b.apply_control_map(&map());
        assert_eq!(b, before);
    }

    #[test]
    fn a_control_map_that_binds_a_key_overrides_the_default() {
        let mut b = Bindings::default();
        assert_eq!(b.get(Action::Activate), Some(&Binding::key(vk::E)));
        let custom = ControlMap::parse("Activate\t0x54\t0xff\t0xff\t1\t1\t1\t0x804");
        b.apply_control_map(&custom);
        assert_eq!(b.get(Action::Activate), Some(&Binding::key(vk::T)));
    }

    fn keys(down: &[u32]) -> impl Fn(u32) -> bool + '_ {
        move |k| down.contains(&k)
    }

    #[test]
    fn holding_w_asks_to_go_forward() {
        let b = Bindings::default();
        let s = InputState::gather(&b, &keys(&[vk::W]), &keys(&[]), 0.0, 0.0);
        assert_eq!(s.move_y, 1.0);
        assert_eq!(s.move_x, 0.0);
        assert!(s.is_moving());
    }

    #[test]
    fn opposite_keys_cancel() {
        let b = Bindings::default();
        let s = InputState::gather(&b, &keys(&[vk::W, vk::S]), &keys(&[]), 0.0, 0.0);
        assert_eq!(s.move_y, 0.0);
        assert!(!s.is_moving());
    }

    #[test]
    fn a_press_is_an_edge_not_a_hold() {
        let b = Bindings::default();
        let held = keys(&[vk::SPACE]);
        let fresh = InputState::gather(&b, &held, &keys(&[]), 0.0, 0.0);
        assert!(fresh.jump_pressed);
        let continued = InputState::gather(&b, &held, &held, 0.0, 0.0);
        assert!(!continued.jump_pressed, "a held key kept firing");
    }

    fn drive(driver: &mut Driver, input: InputState) -> Intent {
        driver.update(&input, &CameraRig::default())
    }

    fn forward() -> InputState {
        InputState {
            move_y: 1.0,
            ..Default::default()
        }
    }

    #[test]
    fn standing_still_asks_for_no_speed() {
        let mut d = Driver::new();
        let i = drive(&mut d, InputState::default());
        assert_eq!(i.speed, 0.0);
        assert_eq!(i.direction, Vec3::new(0.0, 0.0, 0.0));
    }

    #[test]
    fn always_run_is_on_so_a_fresh_player_jogs() {
        let mut d = Driver::new();
        assert_eq!(drive(&mut d, forward()).speed, JOG_SPEED);
    }

    #[test]
    fn the_walk_key_inverts_always_run_both_ways() {
        let mut d = Driver::new();
        let walking = InputState {
            walk: true,
            ..forward()
        };
        assert_eq!(drive(&mut d, walking).speed, FAST_WALK_SPEED);
        d.always_run = false;
        assert_eq!(drive(&mut d, forward()).speed, FAST_WALK_SPEED);
        assert_eq!(drive(&mut d, walking).speed, JOG_SPEED);
    }

    #[test]
    fn sprint_beats_the_walk_and_run_pair() {
        let mut d = Driver::new();
        let sprinting = InputState {
            sprint: true,
            walk: true,
            ..forward()
        };
        assert_eq!(drive(&mut d, sprinting).speed, RUN_SPEED);
    }

    #[test]
    fn sneaking_is_slowest_and_beats_sprint() {
        let mut d = Driver::new();
        d.sneaking = true;
        let sprinting = InputState {
            sprint: true,
            ..forward()
        };
        assert_eq!(drive(&mut d, sprinting).speed, WALK_SPEED);
    }

    #[test]
    fn sneak_toggles_rather_than_holds() {
        let mut d = Driver::new();
        let press = InputState {
            sneak_pressed: true,
            ..Default::default()
        };
        assert!(drive(&mut d, press).crouch);
        // The key going up changes nothing.
        assert!(drive(&mut d, InputState::default()).crouch);
        // Pressing again stands up.
        assert!(!drive(&mut d, press).crouch);
    }

    #[test]
    fn always_run_toggles() {
        let mut d = Driver::new();
        let press = InputState {
            toggle_always_run_pressed: true,
            ..forward()
        };
        assert_eq!(drive(&mut d, press).speed, FAST_WALK_SPEED);
        assert_eq!(drive(&mut d, press).speed, JOG_SPEED);
    }

    #[test]
    fn a_sneaking_character_cannot_jump() {
        let mut d = Driver::new();
        d.sneaking = true;
        let jump = InputState {
            jump_pressed: true,
            ..Default::default()
        };
        assert!(!drive(&mut d, jump).jump);
        d.sneaking = false;
        assert!(drive(&mut d, jump).jump);
    }

    #[test]
    fn movement_follows_the_camera() {
        let mut d = Driver::new();
        let camera = CameraRig::default();
        let east = {
            let mut c = camera;
            c.set_facing(std::f32::consts::FRAC_PI_2, 0.0);
            c
        };
        let north = d.update(&forward(), &camera).direction;
        let turned = d.update(&forward(), &east).direction;
        assert!((north.y - 1.0).abs() < 1e-6, "{north:?}");
        assert!((turned.x - 1.0).abs() < 1e-6, "{turned:?}");
    }

    #[test]
    fn the_direction_stays_normalised_on_a_diagonal() {
        let mut d = Driver::new();
        let diagonal = InputState {
            move_x: 1.0,
            move_y: 1.0,
            ..Default::default()
        };
        let v = drive(&mut d, diagonal).direction;
        assert!(((v.x * v.x + v.y * v.y).sqrt() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn letters_and_space_need_no_translation() {
        for (vk_code, want) in [(vk::W, 87), (vk::A, 65), (vk::S, 83), (vk::SPACE, 32)] {
            assert_eq!(godot_key_for(vk_code), Some(want), "vk {vk_code:#x}");
        }
    }

    #[test]
    fn modifiers_do_need_translation() {
        // The bug this guards against is passing 0x11 straight through, which
        // Godot reads as a control character rather than as Ctrl.
        assert_eq!(godot_key_for(vk::CONTROL), Some(godot_key::CTRL));
        assert_eq!(godot_key_for(vk::SHIFT), Some(godot_key::SHIFT));
        assert_eq!(godot_key_for(vk::ALT), Some(godot_key::ALT));
        assert_ne!(godot_key_for(vk::CONTROL), Some(i64::from(vk::CONTROL)));
    }

    #[test]
    fn sided_modifiers_collapse_onto_the_plain_one() {
        for sided in [0xa0, 0xa1] {
            assert_eq!(godot_key_for(sided), Some(godot_key::SHIFT));
        }
        for sided in [0xa2, 0xa3] {
            assert_eq!(godot_key_for(sided), Some(godot_key::CTRL));
        }
    }

    #[test]
    fn the_function_keys_line_up_across_both_tables() {
        assert_eq!(godot_key_for(0x70), Some(4_194_332));
        assert_eq!(godot_key_for(0x71), Some(4_194_333));
        assert_eq!(godot_key_for(0x7b), Some(4_194_343));
    }

    #[test]
    fn the_arrows_and_the_console_key_translate() {
        assert_eq!(godot_key_for(0x25), Some(godot_key::LEFT));
        assert_eq!(godot_key_for(0x26), Some(godot_key::UP));
        assert_eq!(godot_key_for(0x27), Some(godot_key::RIGHT));
        assert_eq!(godot_key_for(0x28), Some(godot_key::DOWN));
        assert_eq!(godot_key_for(vk::GRAVE), Some(godot_key::QUOTELEFT));
    }

    #[test]
    fn an_unknown_code_translates_to_nothing_rather_than_something_wrong() {
        assert_eq!(godot_key_for(0xfe), None);
        assert_eq!(godot_key_for(UNMAPPED), None);
    }

    #[test]
    fn every_default_binding_reaches_a_real_godot_key() {
        let b = Bindings::default();
        for a in Action::all() {
            for chord in &b.get(a).expect("a default").0 {
                for key in &chord.0 {
                    assert!(
                        godot_key_for(*key).is_some(),
                        "{a:?} binds {key:#x}, which does not translate"
                    );
                }
            }
        }
    }

    #[test]
    fn activate_passes_straight_through() {
        let mut d = Driver::new();
        let press = InputState {
            activate_pressed: true,
            ..Default::default()
        };
        assert!(drive(&mut d, press).activate);
        assert!(!drive(&mut d, InputState::default()).activate);
    }
}
