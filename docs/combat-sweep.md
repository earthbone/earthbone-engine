# combat sweep

WEAP records, fCombatSneak settings and CK wiki combat rules, no captured vanilla runtime oracle

A pass covers the stated observation. Archetypes without a model are reported as no outcome, never as passes. Visual checks stay not run until the installed Godot pass reports them.

| Base | pass | wrong outcome | no outcome | error |
| --- | ---: | ---: | ---: | ---: |
| ARMO | 1 | 0 | 0 | 0 |
| WEAP | 84 | 17 | 18 | 9 |

| Failure cause | Rows |
| --- | ---: |
| hit_unsupported_combat_behavior_non_melee_weapon_or | 6 |
| hit_unsupported_combat_behavior_unarmed_power_attack | 1 |
| kill_lethal_hit_survived | 17 |
| release_weapon_is_not_equipped_or_present_in_the_attacke | 2 |
| weapon_block_not_evaluated | 18 |

Visual checks not passed: 5.

| Visual cause | Checks |
| --- | ---: |
| godot_pass_not_run | 5 |
