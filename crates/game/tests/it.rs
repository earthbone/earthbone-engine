// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::duplicate_mod)]

//! Integration-test registration from the TST-16 backlog contract.
//! Nested scenario modules retain their existing test reruns.

mod common;
mod support;

#[path = "cell_attach_binding.rs"]
mod cell_attach_binding;

#[path = "actor_attributes_adversarial.rs"]
mod actor_attributes_adversarial;
#[path = "actor_skills_adversarial.rs"]
mod actor_skills_adversarial;
#[path = "actor_templates.rs"]
mod actor_templates;
#[path = "actor_values.rs"]
mod actor_values;
#[path = "actor_values_live.rs"]
mod actor_values_live;
#[path = "ai_04_package_fragments.rs"]
mod ai_04_package_fragments;
#[path = "ai_05_breaker_adversarial.rs"]
mod ai_05_breaker_adversarial;
#[path = "ai_05_package_flags.rs"]
mod ai_05_package_flags;
#[path = "ai_09_adversarial.rs"]
mod ai_09_adversarial;
#[path = "ai_09_door_links.rs"]
mod ai_09_door_links;
#[path = "ai_12_adversarial.rs"]
mod ai_12_adversarial;
#[path = "ai_12_alias_fills.rs"]
mod ai_12_alias_fills;
#[path = "ai_14_adversarial.rs"]
mod ai_14_adversarial;
#[path = "ai_14_breaker_adversarial.rs"]
mod ai_14_breaker_adversarial;
#[path = "ai_14_breaker_adversarial_2.rs"]
mod ai_14_breaker_adversarial_2;
#[path = "ai_15_adversarial.rs"]
mod ai_15_adversarial;
#[path = "ai_16_adversarial.rs"]
mod ai_16_adversarial;
#[path = "ai_16_package_boundary_actions.rs"]
mod ai_16_package_boundary_actions;
#[path = "ai_18_adversarial.rs"]
mod ai_18_adversarial;
#[path = "ai_18_location_aliases.rs"]
mod ai_18_location_aliases;
#[path = "ai_19_adversarial.rs"]
mod ai_19_adversarial;
#[path = "ai_19_aliases.rs"]
mod ai_19_aliases;
#[path = "ai_20_adversarial.rs"]
mod ai_20_adversarial;
#[path = "ai_21_adversarial.rs"]
mod ai_21_adversarial;
#[path = "ai_21_hold_position.rs"]
mod ai_21_hold_position;
#[path = "ai_22_adversarial.rs"]
mod ai_22_adversarial;
#[path = "ai_22_follow.rs"]
mod ai_22_follow;
#[path = "ai_23_adversarial.rs"]
mod ai_23_adversarial;
#[path = "ai_24_adversarial.rs"]
mod ai_24_adversarial;
#[path = "ai_25_adversarial.rs"]
mod ai_25_adversarial;
#[path = "ai_25_orbit.rs"]
mod ai_25_orbit;
#[path = "ai_26_adversarial.rs"]
mod ai_26_adversarial;
#[path = "ai_26_escort.rs"]
mod ai_26_escort;
#[path = "ai_27_activate.rs"]
mod ai_27_activate;
#[path = "ai_27_adversarial.rs"]
mod ai_27_adversarial;
#[path = "ai_28_adversarial.rs"]
mod ai_28_adversarial;
#[path = "ai_28_force_greet.rs"]
mod ai_28_force_greet;
#[path = "ai_29_adversarial.rs"]
mod ai_29_adversarial;
#[path = "ai_30_adversarial.rs"]
mod ai_30_adversarial;
#[path = "ai_30_guard.rs"]
mod ai_30_guard;
#[path = "ai_31_adversarial.rs"]
mod ai_31_adversarial;
#[path = "ai_32_adversarial.rs"]
mod ai_32_adversarial;
#[path = "ai_33_adversarial.rs"]
mod ai_33_adversarial;
#[path = "ai_33_find.rs"]
mod ai_33_find;
#[path = "ai_35_adversarial.rs"]
mod ai_35_adversarial;
#[path = "ai_35_dialogue_activate.rs"]
mod ai_35_dialogue_activate;
#[path = "ai_36_adversarial.rs"]
mod ai_36_adversarial;
#[path = "ai_37_adversarial.rs"]
mod ai_37_adversarial;
#[path = "ai_37_conditions.rs"]
mod ai_37_conditions;
#[path = "ai_38_adversarial.rs"]
mod ai_38_adversarial;
#[path = "ai_38_combat_conditions.rs"]
mod ai_38_combat_conditions;
#[path = "ai_39_adversarial.rs"]
mod ai_39_adversarial;
#[path = "ai_39_combat_conditions.rs"]
mod ai_39_combat_conditions;
#[path = "ai_40_adversarial.rs"]
mod ai_40_adversarial;
#[path = "ai_40_combat_conditions.rs"]
mod ai_40_combat_conditions;
#[path = "ai_41_adversarial.rs"]
mod ai_41_adversarial;
#[path = "ai_41_combat_conditions.rs"]
mod ai_41_combat_conditions;
#[path = "ai_42_adversarial.rs"]
mod ai_42_adversarial;
#[path = "ai_43_adversarial.rs"]
mod ai_43_adversarial;
#[path = "ai_43_crime_conditions.rs"]
mod ai_43_crime_conditions;
#[path = "ai_44_adversarial.rs"]
mod ai_44_adversarial;
#[path = "ai_44_crime_conditions.rs"]
mod ai_44_crime_conditions;
#[path = "ai_45_adversarial.rs"]
mod ai_45_adversarial;
#[path = "ai_45_crime_conditions.rs"]
mod ai_45_crime_conditions;
#[path = "ai_46_adversarial.rs"]
mod ai_46_adversarial;
#[path = "ai_46_crime_conditions.rs"]
mod ai_46_crime_conditions;
#[path = "ai_47_adversarial.rs"]
mod ai_47_adversarial;
#[path = "ai_48_adversarial.rs"]
mod ai_48_adversarial;
#[path = "ai_48_dialogue_conditions.rs"]
mod ai_48_dialogue_conditions;
#[path = "ai_49_adversarial.rs"]
mod ai_49_adversarial;
#[path = "ai_49_equipment_conditions.rs"]
mod ai_49_equipment_conditions;
#[path = "ai_50_adversarial.rs"]
mod ai_50_adversarial;
#[path = "ai_51_adversarial.rs"]
mod ai_51_adversarial;
#[path = "ai_51_equipment_conditions.rs"]
mod ai_51_equipment_conditions;
#[path = "ai_52_adversarial.rs"]
mod ai_52_adversarial;
#[path = "ai_53_adversarial.rs"]
mod ai_53_adversarial;
#[path = "ai_53_legacy_conditions.rs"]
mod ai_53_legacy_conditions;
#[path = "ai_54_adversarial.rs"]
mod ai_54_adversarial;
#[path = "ai_54_location_conditions.rs"]
mod ai_54_location_conditions;
#[path = "ai_55_adversarial.rs"]
mod ai_55_adversarial;
#[path = "ai_55_location_conditions.rs"]
mod ai_55_location_conditions;
#[path = "ai_56_adversarial.rs"]
mod ai_56_adversarial;
#[path = "ai_56_location_conditions.rs"]
mod ai_56_location_conditions;
#[path = "ai_57_adversarial.rs"]
mod ai_57_adversarial;
#[path = "ai_57_location_conditions.rs"]
mod ai_57_location_conditions;
#[path = "ai_58_adversarial.rs"]
mod ai_58_adversarial;
#[path = "ai_58_location_conditions.rs"]
mod ai_58_location_conditions;
#[path = "ai_59_adversarial.rs"]
mod ai_59_adversarial;
#[path = "ai_59_magic_conditions.rs"]
mod ai_59_magic_conditions;
#[path = "ai_60_adversarial.rs"]
mod ai_60_adversarial;
#[path = "ai_60_magic_conditions.rs"]
mod ai_60_magic_conditions;
#[path = "ai_61_adversarial.rs"]
mod ai_61_adversarial;
#[path = "ai_61_magic_conditions.rs"]
mod ai_61_magic_conditions;
#[path = "ai_62_adversarial.rs"]
mod ai_62_adversarial;
#[path = "ai_62_motion_conditions.rs"]
mod ai_62_motion_conditions;
#[path = "ai_63_adversarial.rs"]
mod ai_63_adversarial;
#[path = "ai_63_conditions.rs"]
mod ai_63_conditions;
#[path = "ai_64_adversarial.rs"]
mod ai_64_adversarial;
#[path = "ai_64_motion_conditions.rs"]
mod ai_64_motion_conditions;
#[path = "ai_65_adversarial.rs"]
mod ai_65_adversarial;
#[path = "ai_65_breaker2_adversarial.rs"]
mod ai_65_breaker2_adversarial;
#[path = "ai_65_breaker_adversarial.rs"]
mod ai_65_breaker_adversarial;
#[path = "ai_65_conditions.rs"]
mod ai_65_conditions;
#[path = "ai_66_adversarial.rs"]
mod ai_66_adversarial;
#[path = "ai_66_breaker_adversarial.rs"]
mod ai_66_breaker_adversarial;
#[path = "ai_66_motion_conditions.rs"]
mod ai_66_motion_conditions;
#[path = "ai_67_adversarial.rs"]
mod ai_67_adversarial;
#[path = "ai_67_breaker2_adversarial.rs"]
mod ai_67_breaker2_adversarial;
#[path = "ai_67_breaker_adversarial.rs"]
mod ai_67_breaker_adversarial;
#[path = "ai_67_motion_conditions.rs"]
mod ai_67_motion_conditions;
#[path = "ai_68_adversarial.rs"]
mod ai_68_adversarial;
#[path = "ai_68_flying_conditions.rs"]
mod ai_68_flying_conditions;
#[path = "ai_69_adversarial.rs"]
mod ai_69_adversarial;
#[path = "ai_69_quest_conditions.rs"]
mod ai_69_quest_conditions;
#[path = "ai_70_adversarial.rs"]
mod ai_70_adversarial;
#[path = "ai_70_reference_state_conditions.rs"]
mod ai_70_reference_state_conditions;
#[path = "ai_71_adversarial.rs"]
mod ai_71_adversarial;
#[path = "ai_71_reference_conditions.rs"]
mod ai_71_reference_conditions;
#[path = "ai_72_adversarial.rs"]
mod ai_72_adversarial;
#[path = "ai_72_reference_state_conditions.rs"]
mod ai_72_reference_state_conditions;
#[path = "ai_73_breaker_adversarial.rs"]
mod ai_73_breaker_adversarial;
#[path = "ai_73_reference_conditions.rs"]
mod ai_73_reference_conditions;
#[path = "ai_74_adversarial.rs"]
mod ai_74_adversarial;
#[path = "ai_74_conditions.rs"]
mod ai_74_conditions;
#[path = "ai_75_adversarial.rs"]
mod ai_75_adversarial;
#[path = "ai_75_reference_conditions.rs"]
mod ai_75_reference_conditions;
#[path = "ai_76_adversarial.rs"]
mod ai_76_adversarial;
#[path = "ai_76_breaker2_adversarial.rs"]
mod ai_76_breaker2_adversarial;
#[path = "ai_76_breaker_adversarial.rs"]
mod ai_76_breaker_adversarial;
#[path = "ai_76_reference_conditions.rs"]
mod ai_76_reference_conditions;
#[path = "ai_77_breaker_adversarial.rs"]
mod ai_77_breaker_adversarial;
#[path = "ai_77_reference_conditions.rs"]
mod ai_77_reference_conditions;
#[path = "ai_78_adversarial.rs"]
mod ai_78_adversarial;
#[path = "ai_79_breaker_adversarial.rs"]
mod ai_79_breaker_adversarial;
#[path = "ai_79_relationship_conditions.rs"]
mod ai_79_relationship_conditions;
#[path = "ai_80_breaker_adversarial.rs"]
mod ai_80_breaker_adversarial;
#[path = "ai_80_player_conditions.rs"]
mod ai_80_player_conditions;
#[path = "ai_81_breaker_adversarial.rs"]
mod ai_81_breaker_adversarial;
#[path = "ai_82_breaker_adversarial.rs"]
mod ai_82_breaker_adversarial;
#[path = "ai_82_relationship_conditions.rs"]
mod ai_82_relationship_conditions;
#[path = "ai_83_breaker_adversarial.rs"]
mod ai_83_breaker_adversarial;
#[path = "ai_83_relationship_conditions.rs"]
mod ai_83_relationship_conditions;
#[path = "ai_84_breaker_adversarial.rs"]
mod ai_84_breaker_adversarial;
#[path = "ai_84_world_conditions.rs"]
mod ai_84_world_conditions;
#[path = "ai_85_adversarial.rs"]
mod ai_85_adversarial;
#[path = "ai_85_conditions_batch_2.rs"]
mod ai_85_conditions_batch_2;
#[path = "ai_86_adversarial.rs"]
mod ai_86_adversarial;
#[path = "ai_86_condition_contracts.rs"]
mod ai_86_condition_contracts;
#[path = "ai_87_adversarial.rs"]
mod ai_87_adversarial;
#[path = "ai_89_adversarial.rs"]
mod ai_89_adversarial;
#[path = "ai_89_mounted_patrol.rs"]
mod ai_89_mounted_patrol;
#[path = "ai_90_adversarial.rs"]
mod ai_90_adversarial;
#[path = "ai_91_adversarial.rs"]
mod ai_91_adversarial;
#[path = "anm_09_adversarial.rs"]
mod anm_09_adversarial;
#[path = "anm_09_oar_runtime.rs"]
mod anm_09_oar_runtime;
#[path = "anm_55_race_animation_models.rs"]
mod anm_55_race_animation_models;
#[path = "anm_58_adversarial.rs"]
mod anm_58_adversarial;
#[path = "anm_59_expression.rs"]
mod anm_59_expression;
#[path = "atomic_save.rs"]
mod atomic_save;
#[path = "aud_05_adversarial.rs"]
mod aud_05_adversarial;
#[path = "aud_05_music.rs"]
mod aud_05_music;
#[path = "aud_09_adversarial.rs"]
mod aud_09_adversarial;
#[path = "aud_09_ambience.rs"]
mod aud_09_ambience;
#[path = "cmb_05_block_adversarial.rs"]
mod cmb_05_block_adversarial;
#[path = "cmb_06_adversarial.rs"]
mod cmb_06_adversarial;
#[path = "cmb_06_sneak_attack.rs"]
mod cmb_06_sneak_attack;
#[path = "cmb_08_adversarial.rs"]
mod cmb_08_adversarial;
#[path = "cmb_08_projectiles.rs"]
mod cmb_08_projectiles;
#[path = "cmb_09_perk_hit_damage.rs"]
mod cmb_09_perk_hit_damage;
#[path = "cmb_10_quality.rs"]
mod cmb_10_quality;
#[path = "cmb_10_quality_adversarial.rs"]
mod cmb_10_quality_adversarial;
#[path = "cmb_14_stagger.rs"]
mod cmb_14_stagger;
#[path = "cmb_15_adversarial.rs"]
mod cmb_15_adversarial;
#[path = "cmb_15_breaker2_adversarial.rs"]
mod cmb_15_breaker2_adversarial;
#[path = "cmb_15_breaker_adversarial.rs"]
mod cmb_15_breaker_adversarial;
#[path = "cmb_17_adversarial.rs"]
mod cmb_17_adversarial;
#[path = "cmb_17_combat_engagement.rs"]
mod cmb_17_combat_engagement;
#[path = "cmb_18_adversarial.rs"]
mod cmb_18_adversarial;
#[path = "cmb_18_assault.rs"]
mod cmb_18_assault;
#[path = "cmb_20_bash.rs"]
mod cmb_20_bash;
#[path = "cmb_20_bash_adversarial.rs"]
mod cmb_20_bash_adversarial;
#[path = "cmb_21_power_attack.rs"]
mod cmb_21_power_attack;
#[path = "cmb_21_power_attack_adversarial.rs"]
mod cmb_21_power_attack_adversarial;
#[path = "cmb_22_flight.rs"]
mod cmb_22_flight;
#[path = "cmb_23_adversarial.rs"]
mod cmb_23_adversarial;
#[path = "cmb_23_recovered_arrows.rs"]
mod cmb_23_recovered_arrows;
#[path = "cmb_24_adversarial.rs"]
mod cmb_24_adversarial;
#[path = "cmb_24_weapon_enchantment.rs"]
mod cmb_24_weapon_enchantment;
#[path = "cmb_25_weapon_poison.rs"]
mod cmb_25_weapon_poison;
#[path = "cmb_25_weapon_poison_adversarial.rs"]
mod cmb_25_weapon_poison_adversarial;
#[path = "cmb_26_resistance_adversarial.rs"]
mod cmb_26_resistance_adversarial;
#[path = "cmb_26_weapon_resistance.rs"]
mod cmb_26_weapon_resistance;
#[path = "cmb_28_contact_actors.rs"]
mod cmb_28_contact_actors;
#[path = "cmb_31_detection_save_round_trip_adversarial.rs"]
mod cmb_31_detection_save_round_trip_adversarial;
#[path = "cmb_31_shared_detection.rs"]
mod cmb_31_shared_detection;
#[path = "cmb_31_shared_detection_adversarial.rs"]
mod cmb_31_shared_detection_adversarial;
#[path = "cmb_32_detection_events.rs"]
mod cmb_32_detection_events;
#[path = "cmb_33_actor_controller_scale.rs"]
mod cmb_33_actor_controller_scale;
#[path = "cmb_34_player_kill_exception_adversarial.rs"]
mod cmb_34_player_kill_exception_adversarial;
#[path = "cmb_34_protected_state_sync_adversarial.rs"]
mod cmb_34_protected_state_sync_adversarial;
#[path = "cmb_35_magic_recover_bleedout_adversarial.rs"]
mod cmb_35_magic_recover_bleedout_adversarial;
#[path = "cmb_35_magic_scripted_bleedout.rs"]
mod cmb_35_magic_scripted_bleedout;
#[path = "cmb_35_script_damage_bleedout_adversarial.rs"]
mod cmb_35_script_damage_bleedout_adversarial;
#[path = "cmb_36_lazy_actor_base_save_contract.rs"]
mod cmb_36_lazy_actor_base_save_contract;
#[path = "cmb_36_lazy_actor_base_save_state_adversarial.rs"]
mod cmb_36_lazy_actor_base_save_state_adversarial;
#[path = "cmb_37_breaker_adversarial.rs"]
mod cmb_37_breaker_adversarial;
#[path = "cmb_37_destruction.rs"]
mod cmb_37_destruction;
#[path = "cmb_38_adversarial.rs"]
mod cmb_38_adversarial;
#[path = "combat_ai.rs"]
mod combat_ai;
#[path = "combat_bleedout.rs"]
mod combat_bleedout;
#[path = "combat_bleedout_recovery.rs"]
mod combat_bleedout_recovery;
#[path = "combat_bleedout_recovery_adversarial.rs"]
mod combat_bleedout_recovery_adversarial;
#[path = "combat_hits.rs"]
mod combat_hits;
#[path = "combat_lifecycle.rs"]
mod combat_lifecycle;
#[path = "combat_protected_player_kill.rs"]
mod combat_protected_player_kill;
#[path = "combat_stagger_adversarial.rs"]
mod combat_stagger_adversarial;
#[path = "condition_actor_values.rs"]
mod condition_actor_values;
#[path = "condition_ai_50.rs"]
mod condition_ai_50;
#[path = "condition_ai_52_legacy.rs"]
mod condition_ai_52_legacy;
#[path = "condition_compatibility.rs"]
mod condition_compatibility;
#[path = "condition_relationships.rs"]
mod condition_relationships;
#[path = "container_transfer.rs"]
mod container_transfer;
#[path = "coverage.rs"]
mod coverage;
#[path = "crafting.rs"]
mod crafting;
#[path = "crf_01_adversarial.rs"]
mod crf_01_adversarial;
#[path = "crf_01_tempering.rs"]
mod crf_01_tempering;
#[path = "crf_02_adversarial.rs"]
mod crf_02_adversarial;
#[path = "crf_02_alchemy.rs"]
mod crf_02_alchemy;
#[path = "crf_03_adversarial.rs"]
mod crf_03_adversarial;
#[path = "crf_03_disenchant.rs"]
mod crf_03_disenchant;
#[path = "crf_05_adversarial.rs"]
mod crf_05_adversarial;
#[path = "crf_05_breaker3_adversarial.rs"]
mod crf_05_breaker3_adversarial;
#[path = "crf_05_breaker_adversarial.rs"]
mod crf_05_breaker_adversarial;
#[path = "crf_05_breaker_breaker_adversarial.rs"]
mod crf_05_breaker_breaker_adversarial;
#[path = "crf_07_adversarial.rs"]
mod crf_07_adversarial;
#[path = "crf_08_breaker_adversarial.rs"]
mod crf_08_breaker_adversarial;
#[path = "crf_08_crafting_skill.rs"]
mod crf_08_crafting_skill;
#[path = "debug_live.rs"]
mod debug_live;
#[path = "debug_trace.rs"]
mod debug_trace;
#[path = "debug_trace_scr_07_adversarial.rs"]
mod debug_trace_scr_07_adversarial;
#[path = "debug_trace_scr_07_conformance.rs"]
mod debug_trace_scr_07_conformance;
#[path = "debug_trace_scr_07_more_adversarial.rs"]
mod debug_trace_scr_07_more_adversarial;
#[path = "determinism.rs"]
mod determinism;
#[path = "dialogue.rs"]
mod dialogue;
#[path = "dlg_02_adversarial.rs"]
mod dlg_02_adversarial;
#[path = "dlg_02_auto_greetings.rs"]
mod dlg_02_auto_greetings;
#[path = "dlg_03_adversarial.rs"]
mod dlg_03_adversarial;
#[path = "dlg_03_refusal_responses.rs"]
mod dlg_03_refusal_responses;
#[path = "dlg_04_favor_spending.rs"]
mod dlg_04_favor_spending;
#[path = "dlg_07_adversarial.rs"]
mod dlg_07_adversarial;
#[path = "dlg_07_malformed_adversarial.rs"]
mod dlg_07_malformed_adversarial;
#[path = "dlg_07_missing_aidt_adversarial.rs"]
mod dlg_07_missing_aidt_adversarial;
#[path = "dlg_07_persuasion.rs"]
mod dlg_07_persuasion;
#[path = "dlg_08_adversarial.rs"]
mod dlg_08_adversarial;
#[path = "dlg_08_services.rs"]
mod dlg_08_services;
#[path = "dlg_09_adversarial.rs"]
mod dlg_09_adversarial;
#[path = "dlg_09_scene_phases.rs"]
mod dlg_09_scene_phases;
#[path = "dlg_14_adversarial.rs"]
mod dlg_14_adversarial;
#[path = "dlg_14_force_greet_package.rs"]
mod dlg_14_force_greet_package;
#[path = "dlg_15_adversarial.rs"]
mod dlg_15_adversarial;
#[path = "dlg_16_adversarial.rs"]
mod dlg_16_adversarial;
#[path = "dlg_16_training.rs"]
mod dlg_16_training;
#[path = "dlg_17_adversarial.rs"]
mod dlg_17_adversarial;
#[path = "dlg_17_follower_services.rs"]
mod dlg_17_follower_services;
#[path = "dlg_18_adversarial.rs"]
mod dlg_18_adversarial;
#[path = "dlg_18_scene_dialogue.rs"]
mod dlg_18_scene_dialogue;
#[path = "dlg_19_adversarial.rs"]
mod dlg_19_adversarial;
#[path = "dlg_19_scene_actions.rs"]
mod dlg_19_scene_actions;
#[path = "dlg_21_adversarial.rs"]
mod dlg_21_adversarial;
#[path = "encounter_zones.rs"]
mod encounter_zones;
#[path = "fmt_17_breaker_adversarial.rs"]
mod fmt_17_breaker_adversarial;
#[path = "fmt_17_perk_vmad.rs"]
mod fmt_17_perk_vmad;
#[path = "fmt_18_scene_vmad.rs"]
mod fmt_18_scene_vmad;
#[path = "fmt_30_installed_cache.rs"]
mod fmt_30_installed_cache;
#[path = "footstep_actors.rs"]
mod footstep_actors;
#[path = "footsteps.rs"]
mod footsteps;
#[path = "hst_03_adversarial.rs"]
mod hst_03_adversarial;
#[path = "hst_03_breaker_adversarial.rs"]
mod hst_03_breaker_adversarial;
#[path = "hst_03_lockpicking.rs"]
mod hst_03_lockpicking;
#[path = "hst_04_deleted_reference_sync_adversarial.rs"]
mod hst_04_deleted_reference_sync_adversarial;
#[path = "hst_04_placement_sync_check.rs"]
mod hst_04_placement_sync_check;
#[path = "hst_09_adversarial.rs"]
mod hst_09_adversarial;
#[path = "hst_09_furniture.rs"]
mod hst_09_furniture;
#[path = "hst_10_breaker_adversarial.rs"]
mod hst_10_breaker_adversarial;
#[path = "hst_11_adversarial.rs"]
mod hst_11_adversarial;
#[path = "hst_11_sleep.rs"]
mod hst_11_sleep;
#[path = "hst_12_adversarial.rs"]
mod hst_12_adversarial;
#[path = "hst_12_host_pose.rs"]
mod hst_12_host_pose;
#[path = "hst_13_adversarial.rs"]
mod hst_13_adversarial;
#[path = "hst_13_reference_3d_residency.rs"]
mod hst_13_reference_3d_residency;
#[path = "hst_14_adversarial.rs"]
mod hst_14_adversarial;
#[path = "hst_14_trigger_volumes.rs"]
mod hst_14_trigger_volumes;
#[path = "hst_15_adversarial.rs"]
mod hst_15_adversarial;
#[path = "hst_16_adversarial.rs"]
mod hst_16_adversarial;
#[path = "hst_16_translation.rs"]
mod hst_16_translation;
#[path = "hst_17_camera_controls.rs"]
mod hst_17_camera_controls;
#[path = "hst_18_adversarial.rs"]
mod hst_18_adversarial;
#[path = "hst_19_adversarial.rs"]
mod hst_19_adversarial;
#[path = "hst_19_screenshot_request.rs"]
mod hst_19_screenshot_request;
#[path = "image_space.rs"]
mod image_space;
#[path = "inventory_instances.rs"]
mod inventory_instances;
#[path = "inventory_leveled.rs"]
mod inventory_leveled;
#[path = "inventory_references.rs"]
mod inventory_references;
#[path = "leveled_items.rs"]
mod leveled_items;
#[path = "leveled_spells.rs"]
mod leveled_spells;
#[path = "locks.rs"]
mod locks;
#[path = "mag_02_adversarial.rs"]
mod mag_02_adversarial;
#[path = "mag_03_adversarial.rs"]
mod mag_03_adversarial;
#[path = "mag_09_adversarial.rs"]
mod mag_09_adversarial;
#[path = "mag_10_adversarial.rs"]
mod mag_10_adversarial;
#[path = "mag_12_adversarial.rs"]
mod mag_12_adversarial;
#[path = "mag_13_actor_abilities.rs"]
mod mag_13_actor_abilities;
#[path = "mag_13_adversarial.rs"]
mod mag_13_adversarial;
#[path = "mag_14_adversarial.rs"]
mod mag_14_adversarial;
#[path = "mag_14_breaker_adversarial.rs"]
mod mag_14_breaker_adversarial;
#[path = "mag_14_effect_shader.rs"]
mod mag_14_effect_shader;
#[path = "mag_16_adversarial.rs"]
mod mag_16_adversarial;
#[path = "mag_17_adversarial.rs"]
mod mag_17_adversarial;
#[path = "mag_17_breaker_adversarial.rs"]
mod mag_17_breaker_adversarial;
#[path = "mag_18_adversarial.rs"]
mod mag_18_adversarial;
#[path = "mag_19_adversarial.rs"]
mod mag_19_adversarial;
#[path = "mag_19_taper.rs"]
mod mag_19_taper;
#[path = "mag_20_adversarial.rs"]
mod mag_20_adversarial;
#[path = "mag_20_equip_ability.rs"]
mod mag_20_equip_ability;
#[path = "mag_21_adversarial.rs"]
mod mag_21_adversarial;
#[path = "mag_22_adversarial.rs"]
mod mag_22_adversarial;
#[path = "mag_23_adversarial.rs"]
mod mag_23_adversarial;
#[path = "mag_24_adversarial.rs"]
mod mag_24_adversarial;
#[path = "mag_27_adversarial.rs"]
mod mag_27_adversarial;
#[path = "mag_28_adversarial.rs"]
mod mag_28_adversarial;
#[path = "mag_29_adversarial.rs"]
mod mag_29_adversarial;
#[path = "mag_30_adversarial.rs"]
mod mag_30_adversarial;
#[path = "mag_31_adversarial.rs"]
mod mag_31_adversarial;
#[path = "mag_31_frenzy.rs"]
mod mag_31_frenzy;
#[path = "mag_32_adversarial.rs"]
mod mag_32_adversarial;
#[path = "mag_33_adversarial.rs"]
mod mag_33_adversarial;
#[path = "mag_33_command_summoned.rs"]
mod mag_33_command_summoned;
#[path = "mag_34_adversarial.rs"]
mod mag_34_adversarial;
#[path = "mag_34_breaker_2_adversarial.rs"]
mod mag_34_breaker_2_adversarial;
#[path = "mag_34_breaker_adversarial.rs"]
mod mag_34_breaker_adversarial;
#[path = "mag_35_adversarial.rs"]
mod mag_35_adversarial;
#[path = "mag_35_breaker_2_adversarial.rs"]
mod mag_35_breaker_2_adversarial;
#[path = "mag_35_breaker_adversarial.rs"]
mod mag_35_breaker_adversarial;
#[path = "mag_36_adversarial.rs"]
mod mag_36_adversarial;
#[path = "mag_36_lock.rs"]
mod mag_36_lock;
#[path = "mag_37_adversarial.rs"]
mod mag_37_adversarial;
#[path = "mag_37_open.rs"]
mod mag_37_open;
#[path = "mag_38_adversarial.rs"]
mod mag_38_adversarial;
#[path = "mag_39_summon_adversarial.rs"]
mod mag_39_summon_adversarial;
#[path = "mag_40_adversarial.rs"]
mod mag_40_adversarial;
#[path = "mag_40_detect_life.rs"]
mod mag_40_detect_life;
#[path = "mag_41_adversarial.rs"]
mod mag_41_adversarial;
#[path = "mag_41_breaker_adversarial.rs"]
mod mag_41_breaker_adversarial;
#[path = "mag_41_breaker_extra_adversarial.rs"]
mod mag_41_breaker_extra_adversarial;
#[path = "mag_42_adversarial.rs"]
mod mag_42_adversarial;
#[path = "mag_43_adversarial.rs"]
mod mag_43_adversarial;
#[path = "mag_43_reanimate.rs"]
mod mag_43_reanimate;
#[path = "mag_44_adversarial.rs"]
mod mag_44_adversarial;
#[path = "mag_44_soul_trap.rs"]
mod mag_44_soul_trap;
#[path = "mag_45_adversarial.rs"]
mod mag_45_adversarial;
#[path = "mag_45_turn_undead.rs"]
mod mag_45_turn_undead;
#[path = "mag_46_adversarial.rs"]
mod mag_46_adversarial;
#[path = "mag_46_guide.rs"]
mod mag_46_guide;
#[path = "mag_47_adversarial.rs"]
mod mag_47_adversarial;
#[path = "mag_48_adversarial.rs"]
mod mag_48_adversarial;
#[path = "mag_49_adversarial.rs"]
mod mag_49_adversarial;
#[path = "mag_50_adversarial.rs"]
mod mag_50_adversarial;
#[path = "mag_50_cure_poison.rs"]
mod mag_50_cure_poison;
#[path = "mag_51_adversarial.rs"]
mod mag_51_adversarial;
#[path = "mag_51_concussion.rs"]
mod mag_51_concussion;
#[path = "mag_52_adversarial.rs"]
mod mag_52_adversarial;
#[path = "mag_53_accumulate_magnitude.rs"]
mod mag_53_accumulate_magnitude;
#[path = "mag_53_adversarial.rs"]
mod mag_53_adversarial;
#[path = "mag_54_adversarial.rs"]
mod mag_54_adversarial;
#[path = "mag_55_adversarial.rs"]
mod mag_55_adversarial;
#[path = "mag_56_adversarial.rs"]
mod mag_56_adversarial;
#[path = "mag_56_cloak.rs"]
mod mag_56_cloak;
#[path = "mag_57_adversarial.rs"]
mod mag_57_adversarial;
#[path = "mag_57_werewolf.rs"]
mod mag_57_werewolf;
#[path = "mag_58_adversarial.rs"]
mod mag_58_adversarial;
#[path = "mag_59_adversarial.rs"]
mod mag_59_adversarial;
#[path = "mag_60_adversarial.rs"]
mod mag_60_adversarial;
#[path = "mag_61_adversarial.rs"]
mod mag_61_adversarial;
#[path = "mag_61_breaker_adversarial.rs"]
mod mag_61_breaker_adversarial;
#[path = "mag_61_breaker_breaker_adversarial.rs"]
mod mag_61_breaker_breaker_adversarial;
#[path = "mag_62_adversarial.rs"]
mod mag_62_adversarial;
#[path = "mag_63_adversarial.rs"]
mod mag_63_adversarial;
#[path = "mag_64_adversarial.rs"]
mod mag_64_adversarial;
#[path = "mag_65_adversarial.rs"]
mod mag_65_adversarial;
#[path = "mag_66_breaker_adversarial.rs"]
mod mag_66_breaker_adversarial;
#[path = "mag_66_grab_actor.rs"]
mod mag_66_grab_actor;
#[path = "mag_67_adversarial.rs"]
mod mag_67_adversarial;
#[path = "mag_69_adversarial.rs"]
mod mag_69_adversarial;
#[path = "mag_70_adversarial.rs"]
mod mag_70_adversarial;
#[path = "mag_72.rs"]
mod mag_72;
#[path = "mag_73_adversarial.rs"]
mod mag_73_adversarial;
#[path = "magic.rs"]
mod magic;
#[path = "magic_archetypes.rs"]
mod magic_archetypes;
#[path = "mod_01_adversarial.rs"]
mod mod_01_adversarial;
#[path = "mod_01_breaker2_adversarial.rs"]
mod mod_01_breaker2_adversarial;
#[path = "mod_01_breaker_adversarial.rs"]
mod mod_01_breaker_adversarial;
#[path = "mod_02_adversarial.rs"]
mod mod_02_adversarial;
#[path = "mod_03_adversarial.rs"]
mod mod_03_adversarial;
#[path = "mod_04_breaker_2_adversarial.rs"]
mod mod_04_breaker_2_adversarial;
#[path = "mod_05_adversarial.rs"]
mod mod_05_adversarial;
#[path = "mod_06_adversarial.rs"]
mod mod_06_adversarial;
#[path = "mod_08_adversarial.rs"]
mod mod_08_adversarial;
#[path = "mod_08_distributed_shouts.rs"]
mod mod_08_distributed_shouts;
#[path = "mod_09_adversarial.rs"]
mod mod_09_adversarial;
#[path = "mod_10_adversarial.rs"]
mod mod_10_adversarial;
#[path = "moddata_apply.rs"]
mod moddata_apply;
#[path = "natives_2.rs"]
mod natives_2;
#[path = "natives_dlc.rs"]
mod natives_dlc;
#[path = "nearby_interior_destination_adversarial.rs"]
mod nearby_interior_destination_adversarial;
#[path = "no_load_screen.rs"]
mod no_load_screen;
#[path = "no_load_screen_live.rs"]
mod no_load_screen_live;
#[path = "npc.rs"]
mod npc;
#[path = "outfits.rs"]
mod outfits;
#[path = "package_hover.rs"]
mod package_hover;
#[path = "package_patrol.rs"]
mod package_patrol;
#[path = "package_sequences.rs"]
mod package_sequences;
#[path = "package_simultaneous.rs"]
mod package_simultaneous;
#[path = "package_wait.rs"]
mod package_wait;
#[path = "packages.rs"]
mod packages;
#[path = "perk_dispatch.rs"]
mod perk_dispatch;
#[path = "perks.rs"]
mod perks;
#[path = "prf_09_adversarial.rs"]
mod prf_09_adversarial;
#[path = "prf_09_profiling.rs"]
mod prf_09_profiling;
#[path = "prf_10_adversarial.rs"]
mod prf_10_adversarial;
#[path = "prf_10_budgets.rs"]
mod prf_10_budgets;
#[path = "prg_01_adversarial.rs"]
mod prg_01_adversarial;
#[path = "prg_01_breaker_adversarial.rs"]
mod prg_01_breaker_adversarial;
#[path = "prg_01_skill_use.rs"]
mod prg_01_skill_use;
#[path = "prg_03_actor_attributes.rs"]
mod prg_03_actor_attributes;
#[path = "prg_06_witnessed_crime.rs"]
mod prg_06_witnessed_crime;
#[path = "prg_06_witnessed_crime_adversarial.rs"]
mod prg_06_witnessed_crime_adversarial;
#[path = "prg_07_adversarial.rs"]
mod prg_07_adversarial;
#[path = "prg_07_character_level.rs"]
mod prg_07_character_level;
#[path = "prg_08_adversarial.rs"]
mod prg_08_adversarial;
#[path = "prg_09_adversarial.rs"]
mod prg_09_adversarial;
#[path = "prg_09_race_skill_initialization.rs"]
mod prg_09_race_skill_initialization;
#[path = "prg_10_adversarial.rs"]
mod prg_10_adversarial;
#[path = "prg_12_adversarial.rs"]
mod prg_12_adversarial;
#[path = "prg_12_arrest_choices.rs"]
mod prg_12_arrest_choices;
#[path = "prg_13_adversarial.rs"]
mod prg_13_adversarial;
#[path = "prg_13_jail.rs"]
mod prg_13_jail;
#[path = "prg_14_adversarial.rs"]
mod prg_14_adversarial;
#[path = "prg_14_breaker_adversarial.rs"]
mod prg_14_breaker_adversarial;
#[path = "prg_14_breaker_adversarial_2.rs"]
mod prg_14_breaker_adversarial_2;
#[path = "prg_14_story_manager.rs"]
mod prg_14_story_manager;
#[path = "prg_15_actor_skills.rs"]
mod prg_15_actor_skills;
#[path = "prg_16_data_length_adversarial.rs"]
mod prg_16_data_length_adversarial;
#[path = "prg_16_purchase_data_semantics.rs"]
mod prg_16_purchase_data_semantics;
#[path = "quest_flow.rs"]
mod quest_flow;
#[path = "real_crafting.rs"]
mod real_crafting;
#[path = "real_files.rs"]
mod real_files;
#[path = "real_transition.rs"]
mod real_transition;
#[path = "sav_01_adversarial.rs"]
mod sav_01_adversarial;
#[path = "sav_04_adversarial.rs"]
mod sav_04_adversarial;
#[path = "sav_06_actor_value_modifiers.rs"]
mod sav_06_actor_value_modifiers;
#[path = "sav_06_adversarial.rs"]
mod sav_06_adversarial;
#[path = "sav_09_adversarial.rs"]
mod sav_09_adversarial;
#[path = "sav_10_adversarial.rs"]
mod sav_10_adversarial;
#[path = "sav_15_adversarial.rs"]
mod sav_15_adversarial;
#[path = "sav_15_breaker_adversarial.rs"]
mod sav_15_breaker_adversarial;
#[path = "sav_17_adversarial.rs"]
mod sav_17_adversarial;
#[path = "sav_17_breaker_adversarial.rs"]
mod sav_17_breaker_adversarial;
#[path = "sav_18_breaker_adversarial.rs"]
mod sav_18_breaker_adversarial;
#[path = "sav_18_wait_deadlines.rs"]
mod sav_18_wait_deadlines;
#[path = "sav_20_adversarial.rs"]
mod sav_20_adversarial;
#[path = "sav_24_adversarial.rs"]
mod sav_24_adversarial;
#[path = "sav_28_adversarial.rs"]
mod sav_28_adversarial;
#[path = "sav_28_spatial_restore.rs"]
mod sav_28_spatial_restore;
#[path = "sav_40_adversarial.rs"]
mod sav_40_adversarial;
#[path = "sav_42_adversarial.rs"]
mod sav_42_adversarial;
#[path = "sav_44_adversarial.rs"]
mod sav_44_adversarial;
#[path = "sav_46_adversarial.rs"]
mod sav_46_adversarial;
#[path = "sav_46_breaker_adversarial.rs"]
mod sav_46_breaker_adversarial;
#[path = "sav_48_adversarial.rs"]
mod sav_48_adversarial;
#[path = "sav_50_adversarial.rs"]
mod sav_50_adversarial;
#[path = "sav_52_adversarial.rs"]
mod sav_52_adversarial;
#[path = "sav_54_adversarial.rs"]
mod sav_54_adversarial;
#[path = "sav_54_breaker_adversarial.rs"]
mod sav_54_breaker_adversarial;
#[path = "sav_56_adversarial.rs"]
mod sav_56_adversarial;
#[path = "sav_56_breaker_adversarial.rs"]
mod sav_56_breaker_adversarial;
#[path = "sav_56_provenance.rs"]
mod sav_56_provenance;
#[path = "sav_57_adversarial.rs"]
mod sav_57_adversarial;
#[path = "sav_57_breaker_adversarial.rs"]
mod sav_57_breaker_adversarial;
#[path = "sav_57_preflight.rs"]
mod sav_57_preflight;
#[path = "sav_58_adversarial.rs"]
mod sav_58_adversarial;
#[path = "sav_60_breaker_adversarial.rs"]
mod sav_60_breaker_adversarial;
#[path = "sav_60_reconnect.rs"]
mod sav_60_reconnect;
#[path = "sav_61_adversarial.rs"]
mod sav_61_adversarial;
#[path = "sav_61_recovery_session.rs"]
mod sav_61_recovery_session;
#[path = "sav_63_adversarial.rs"]
mod sav_63_adversarial;
#[path = "sav_68_adversarial.rs"]
mod sav_68_adversarial;
#[path = "sav_68_reset.rs"]
mod sav_68_reset;
#[path = "save_generations.rs"]
mod save_generations;
#[path = "scr_01_adversarial.rs"]
mod scr_01_adversarial;
#[path = "scr_02_adversarial.rs"]
mod scr_02_adversarial;
#[path = "scr_02_native_contract_inventory.rs"]
mod scr_02_native_contract_inventory;
#[path = "scr_03_adversarial.rs"]
mod scr_03_adversarial;
#[path = "scr_03_component.rs"]
mod scr_03_component;
#[path = "scr_05_adversarial.rs"]
mod scr_05_adversarial;
#[path = "scr_05_lazy_cell_binding.rs"]
mod scr_05_lazy_cell_binding;
#[path = "scr_08_actor_native_contracts.rs"]
mod scr_08_actor_native_contracts;
#[path = "scr_08_adversarial.rs"]
mod scr_08_adversarial;
#[path = "scr_09_actor_queries.rs"]
mod scr_09_actor_queries;
#[path = "scr_09_adversarial.rs"]
mod scr_09_adversarial;
#[path = "scr_10_actor_state_natives.rs"]
mod scr_10_actor_state_natives;
#[path = "scr_10_adversarial.rs"]
mod scr_10_adversarial;
#[path = "scr_11_adversarial.rs"]
mod scr_11_adversarial;
#[path = "scr_12_adversarial.rs"]
mod scr_12_adversarial;
#[path = "scr_13_actor_native_batch.rs"]
mod scr_13_actor_native_batch;
#[path = "scr_13_adversarial.rs"]
mod scr_13_adversarial;
#[path = "scr_14_adversarial.rs"]
mod scr_14_adversarial;
#[path = "scr_15_adversarial.rs"]
mod scr_15_adversarial;
#[path = "scr_16_adversarial.rs"]
mod scr_16_adversarial;
#[path = "scr_17_adversarial.rs"]
mod scr_17_adversarial;
#[path = "scr_18_actor_base_gift_filter.rs"]
mod scr_18_actor_base_gift_filter;
#[path = "scr_18_adversarial.rs"]
mod scr_18_adversarial;
#[path = "scr_19_adversarial.rs"]
mod scr_19_adversarial;
#[path = "scr_20_adversarial.rs"]
mod scr_20_adversarial;
#[path = "scr_20_debug_natives.rs"]
mod scr_20_debug_natives;
#[path = "scr_21_adversarial.rs"]
mod scr_21_adversarial;
#[path = "scr_22_adversarial.rs"]
mod scr_22_adversarial;
#[path = "scr_22_debug_profiling.rs"]
mod scr_22_debug_profiling;
#[path = "scr_23_adversarial.rs"]
mod scr_23_adversarial;
#[path = "scr_23_debug_natives.rs"]
mod scr_23_debug_natives;
#[path = "scr_24_adversarial.rs"]
mod scr_24_adversarial;
#[path = "scr_24_effect_shader.rs"]
mod scr_24_effect_shader;
#[path = "scr_25_adversarial.rs"]
mod scr_25_adversarial;
#[path = "scr_26_adversarial.rs"]
mod scr_26_adversarial;
#[path = "scr_27_adversarial.rs"]
mod scr_27_adversarial;
#[path = "scr_28_adversarial.rs"]
mod scr_28_adversarial;
#[path = "scr_29_adversarial.rs"]
mod scr_29_adversarial;
#[path = "scr_29_game_native_contracts.rs"]
mod scr_29_game_native_contracts;
#[path = "scr_30_adversarial.rs"]
mod scr_30_adversarial;
#[path = "scr_30_game_natives.rs"]
mod scr_30_game_natives;
#[path = "scr_31_adversarial.rs"]
mod scr_31_adversarial;
#[path = "scr_32_adversarial.rs"]
mod scr_32_adversarial;
#[path = "scr_32_game_queries.rs"]
mod scr_32_game_queries;
#[path = "scr_33_adversarial.rs"]
mod scr_33_adversarial;
#[path = "scr_34_adversarial.rs"]
mod scr_34_adversarial;
#[path = "scr_34_keyword_story_events.rs"]
mod scr_34_keyword_story_events;
#[path = "scr_35_adversarial.rs"]
mod scr_35_adversarial;
#[path = "scr_36_adversarial.rs"]
mod scr_36_adversarial;
#[path = "scr_37_adversarial.rs"]
mod scr_37_adversarial;
#[path = "scr_37_music_type.rs"]
mod scr_37_music_type;
#[path = "scr_38_adversarial.rs"]
mod scr_38_adversarial;
#[path = "scr_38_object_reference.rs"]
mod scr_38_object_reference;
#[path = "scr_39_breaker_2_adversarial.rs"]
mod scr_39_breaker_2_adversarial;
#[path = "scr_39_breaker_adversarial.rs"]
mod scr_39_breaker_adversarial;
#[path = "scr_39_current_scene.rs"]
mod scr_39_current_scene;
#[path = "scr_39_object_reference_batch_2.rs"]
mod scr_39_object_reference_batch_2;
#[path = "scr_40_adversarial.rs"]
mod scr_40_adversarial;
#[path = "scr_40_object_reference.rs"]
mod scr_40_object_reference;
#[path = "scr_41_adversarial.rs"]
mod scr_41_adversarial;
#[path = "scr_42_adversarial.rs"]
mod scr_42_adversarial;
#[path = "scr_42_object_reference.rs"]
mod scr_42_object_reference;
#[path = "scr_43_adversarial.rs"]
mod scr_43_adversarial;
#[path = "scr_43_quest_transition_natives.rs"]
mod scr_43_quest_transition_natives;
#[path = "scr_44_adversarial.rs"]
mod scr_44_adversarial;
#[path = "scr_44_scene_action_complete.rs"]
mod scr_44_scene_action_complete;
#[path = "scr_45_adversarial.rs"]
mod scr_45_adversarial;
#[path = "scr_46_adversarial.rs"]
mod scr_46_adversarial;
#[path = "scr_46_shader_particle_geometry.rs"]
mod scr_46_shader_particle_geometry;
#[path = "scr_47_adversarial.rs"]
mod scr_47_adversarial;
#[path = "scr_48_adversarial.rs"]
mod scr_48_adversarial;
#[path = "scr_49_adversarial.rs"]
mod scr_49_adversarial;
#[path = "scr_49_utility_frame_rate.rs"]
mod scr_49_utility_frame_rate;
#[path = "scr_50_adversarial.rs"]
mod scr_50_adversarial;
#[path = "scr_50_utility_runtime_queries.rs"]
mod scr_50_utility_runtime_queries;
#[path = "scr_51_adversarial.rs"]
mod scr_51_adversarial;
#[path = "scr_51_utility_budgets.rs"]
mod scr_51_utility_budgets;
#[path = "scr_52_adversarial.rs"]
mod scr_52_adversarial;
#[path = "scr_52_visual_effects.rs"]
mod scr_52_visual_effects;
#[path = "scr_53_adversarial.rs"]
mod scr_53_adversarial;
#[path = "scr_53_weapon_fire.rs"]
mod scr_53_weapon_fire;
#[path = "scr_54_adversarial.rs"]
mod scr_54_adversarial;
#[path = "scr_54_weather_set_active.rs"]
mod scr_54_weather_set_active;
#[path = "scr_55_adversarial.rs"]
mod scr_55_adversarial;
#[path = "scr_56_adversarial.rs"]
mod scr_56_adversarial;
#[path = "scr_57_adversarial.rs"]
mod scr_57_adversarial;
#[path = "scr_57_native_contract_audit.rs"]
mod scr_57_native_contract_audit;
#[path = "scr_58_adversarial.rs"]
mod scr_58_adversarial;
#[path = "scr_59_adversarial.rs"]
mod scr_59_adversarial;
#[path = "scr_60_adversarial.rs"]
mod scr_60_adversarial;
#[path = "scr_61_adversarial.rs"]
mod scr_61_adversarial;
#[path = "scr_61_native_contracts.rs"]
mod scr_61_native_contracts;
#[path = "scr_62_adversarial.rs"]
mod scr_62_adversarial;
#[path = "scr_63_adversarial.rs"]
mod scr_63_adversarial;
#[path = "scr_63_contracts.rs"]
mod scr_63_contracts;
#[path = "scr_64_adversarial.rs"]
mod scr_64_adversarial;
#[path = "scr_65_adversarial.rs"]
mod scr_65_adversarial;
#[path = "scr_66_adversarial.rs"]
mod scr_66_adversarial;
#[path = "scr_67_adversarial.rs"]
mod scr_67_adversarial;
#[path = "scr_67_native_registry.rs"]
mod scr_67_native_registry;
#[path = "scr_68_adversarial.rs"]
mod scr_68_adversarial;
#[path = "scr_68_native_contracts.rs"]
mod scr_68_native_contracts;
#[path = "scr_69_adversarial.rs"]
mod scr_69_adversarial;
#[path = "scr_70_adversarial.rs"]
mod scr_70_adversarial;
#[path = "scr_71_adversarial.rs"]
mod scr_71_adversarial;
#[path = "scr_72_adversarial.rs"]
mod scr_72_adversarial;
#[path = "scr_73_adversarial.rs"]
mod scr_73_adversarial;
#[path = "scr_73_native_contract_audit.rs"]
mod scr_73_native_contract_audit;
#[path = "shm_03_live_world.rs"]
mod shm_03_live_world;
#[path = "shm_04_actor_world_binding.rs"]
mod shm_04_actor_world_binding;
#[path = "shm_06_world.rs"]
mod shm_06_world;
#[path = "shm_07_bindings.rs"]
mod shm_07_bindings;
#[path = "shm_08_live_extra_data.rs"]
mod shm_08_live_extra_data;
#[path = "shm_09_live_extras.rs"]
mod shm_09_live_extras;
#[path = "shm_11_adversarial.rs"]
mod shm_11_adversarial;
#[path = "shm_12_live_world.rs"]
mod shm_12_live_world;
#[path = "shm_14_world_contracts.rs"]
mod shm_14_world_contracts;
#[path = "shm_15_adversarial.rs"]
mod shm_15_adversarial;
#[path = "shm_15_world_state.rs"]
mod shm_15_world_state;
#[path = "shm_16_adversarial.rs"]
mod shm_16_adversarial;
#[path = "shm_17_conditions.rs"]
mod shm_17_conditions;
#[path = "shm_18_adversarial.rs"]
mod shm_18_adversarial;
#[path = "shm_18_live_events.rs"]
mod shm_18_live_events;
#[path = "shm_19_adversarial.rs"]
mod shm_19_adversarial;
#[path = "shm_19_events.rs"]
mod shm_19_events;
#[path = "shm_23_adversarial.rs"]
mod shm_23_adversarial;
#[path = "shm_23_quest_start_stop.rs"]
mod shm_23_quest_start_stop;
#[path = "sound.rs"]
mod sound;
#[path = "tool_01_breaker_adversarial.rs"]
mod tool_01_breaker_adversarial;
#[path = "tool_01_validation_runner.rs"]
mod tool_01_validation_runner;
#[path = "transitions.rs"]
mod transitions;
#[path = "tst_01_adversarial.rs"]
mod tst_01_adversarial;
#[path = "tst_01_rng_streams.rs"]
mod tst_01_rng_streams;
#[cfg(feature = "save-fault-tests")]
#[path = "tst_06_adversarial.rs"]
mod tst_06_adversarial;
#[path = "tst_11_adversarial.rs"]
mod tst_11_adversarial;
#[path = "tst_11_papyrus_log.rs"]
mod tst_11_papyrus_log;
#[path = "tst_12_additional_adversarial.rs"]
mod tst_12_additional_adversarial;
#[path = "tst_12_adversarial.rs"]
mod tst_12_adversarial;
#[path = "tst_12_breaker2_adversarial.rs"]
mod tst_12_breaker2_adversarial;
#[path = "tst_12_breaker_adversarial.rs"]
mod tst_12_breaker_adversarial;
#[path = "tst_12_mod_corpus.rs"]
mod tst_12_mod_corpus;
#[path = "tst_14_adversarial.rs"]
mod tst_14_adversarial;
#[path = "tst_15_adversarial.rs"]
mod tst_15_adversarial;
#[path = "tst_15_legacy.rs"]
mod tst_15_legacy;
#[path = "tst_16_adversarial.rs"]
mod tst_16_adversarial;
#[path = "tst_16_test_binary.rs"]
mod tst_16_test_binary;
#[path = "ui_03_magic_menu.rs"]
mod ui_03_magic_menu;
#[path = "ui_05_adversarial.rs"]
mod ui_05_adversarial;
#[path = "ui_05_mcm.rs"]
mod ui_05_mcm;
#[path = "ui_07_character_traits.rs"]
mod ui_07_character_traits;
#[path = "ui_08_adversarial.rs"]
mod ui_08_adversarial;
#[path = "ui_08_breaker_adversarial.rs"]
mod ui_08_breaker_adversarial;
#[path = "ui_08_lockpicking.rs"]
mod ui_08_lockpicking;
#[path = "ui_09_adversarial.rs"]
mod ui_09_adversarial;
#[path = "ui_09_vendor_trade.rs"]
mod ui_09_vendor_trade;
#[path = "ui_14_player_appearance.rs"]
mod ui_14_player_appearance;
#[path = "ui_15_adversarial.rs"]
mod ui_15_adversarial;
#[path = "ui_15_gift_menu.rs"]
mod ui_15_gift_menu;
#[path = "ui_17_adversarial.rs"]
mod ui_17_adversarial;
#[path = "vanilla_save.rs"]
mod vanilla_save;
#[path = "visual_effects.rs"]
mod visual_effects;
#[path = "anm_59_breaker_adversarial.rs"]
mod anm_59_breaker_adversarial;
