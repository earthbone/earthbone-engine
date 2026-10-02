// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::cell::RefCell;
use std::rc::Rc;

use soe_commonlib_shim::events::{EventSink, FormIdentity, LevelIncrease, NotifyControl};
use soe_esm::LoadOrder;
use soe_game::World;

struct Capture(Rc<RefCell<Vec<LevelIncrease>>>);

impl EventSink<LevelIncrease> for Capture {
    fn process(&mut self, event: &LevelIncrease) -> NotifyControl {
        self.0.borrow_mut().push(*event);
        NotifyControl::Continue
    }
}

#[test]
fn level_increase_uses_current_value_on_each_send_at_u16_limit() {
    let mut world = World::new(LoadOrder::new());
    world.player_level = u32::from(u16::MAX) - 1;
    let seen = Rc::new(RefCell::new(Vec::new()));
    world
        .commonlib_event_sources()
        .level_increase()
        .add_sink(Box::new(Capture(seen.clone())));

    assert_eq!(world.commonlib_level_increase(), Ok(1));
    world.player_level = u32::from(u16::MAX);
    assert_eq!(world.commonlib_level_increase(), Ok(1));
    assert_eq!(
        *seen.borrow(),
        [
            LevelIncrease {
                player: FormIdentity(0x14),
                new_level: u16::MAX - 1,
            },
            LevelIncrease {
                player: FormIdentity(0x14),
                new_level: u16::MAX,
            },
        ]
    );
}

#[test]
fn unsupported_hook_kinds_all_fail_closed() {
    use soe_commonlib_shim::events::{HookError, HookKind, request_hook};

    for kind in [HookKind::CallSite, HookKind::Branch, HookKind::Vtable] {
        assert_eq!(request_hook(kind), Err(HookError::Unsupported(kind)));
    }
}
