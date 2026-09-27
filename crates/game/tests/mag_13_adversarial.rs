// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

#[test]
fn constant_npc_ability_activates_once_at_first_spawn_and_does_not_cast_ordinary_spell() {
    let t = Tag::new;
    let (npc, actor, ability, ordinary, mgef): (u32, u32, u32, u32, u32) = (0x500, 0x100, 0x300, 0x301, 0x200);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(t(b"NPC_"), npc).sub(t(b"ACBS"), vec![0; 24])
        .sub(t(b"SPLO"), ability.to_le_bytes()).sub(t(b"SPLO"), ordinary.to_le_bytes()));
    plugin.push_top(Record::new(t(b"ACHR"), actor).sub(t(b"NAME"), npc.to_le_bytes()).sub(t(b"DATA"), vec![0; 24]));
    let mut mgef_data = vec![0; 152];
    mgef_data[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    mgef_data[68..72].copy_from_slice(&24u32.to_le_bytes());
    plugin.push_top(Record::new(t(b"MGEF"), mgef).sub(t(b"DATA"), mgef_data));
    for (form, spell_type) in [(ability, 4u32), (ordinary, 0u32)] {
        let mut spit = vec![0; 36]; spit[8..12].copy_from_slice(&spell_type.to_le_bytes());
        plugin.push_top(Record::new(t(b"SPEL"), form).sub(t(b"SPIT"), spit)
            .sub(t(b"EFID"), mgef.to_le_bytes()).sub(t(b"EFIT"), [5.0f32.to_le_bytes().as_slice(), &0u32.to_le_bytes(), &0u32.to_le_bytes()].concat()));
    }
    let mut order = LoadOrder::new(); order.add("Abilities.esm", plugin).unwrap();
    let game = Game::new(order);
    let actor_id = GlobalFormId { file: 0, object: actor };
    assert!(game.world_mut().actor(actor_id).is_some());
    game.world_mut().actor(actor_id);
    let world = game.world();
    let active: Vec<_> = world.active_effects().collect();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].spell, GlobalFormId { file: 0, object: ability });
    assert_eq!(active[0].target, actor_id);
}
