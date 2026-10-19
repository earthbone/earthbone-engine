# consumables sweep

CK wiki and UESP archetypes, MGEF flags and ALCH records, no captured vanilla runtime oracle

A pass covers the stated observation. Archetypes without a model are reported as no outcome, never as passes. Visual checks stay not run until the installed Godot pass reports them.

| Base | pass | wrong outcome | no outcome | error |
| --- | ---: | ---: | ---: | ---: |
| ALCH | 0 | 20 | 0 | 0 |
| INGR | 0 | 0 | 20 | 0 |

| Failure cause | Rows |
| --- | ---: |
| ingredient_eat_path_missing | 20 |
| same_potion_stacks | 20 |

Visual checks not passed: 9.

| Visual cause | Checks |
| --- | ---: |
| godot_pass_not_run | 9 |
