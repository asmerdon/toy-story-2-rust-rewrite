# Campaign interaction coverage (levels 2–15)

The single record of what each level's interactions are and how far each has been checked.
Level 1 is signed off by the user and is the regression reference. Update this table when
an interaction is tested, fixed or reported; HANDOFF.md links here instead of repeating it.

Columns:
- **Sim**: an integration test drives it with real game data (`cargo test --release`, `TS2_GAME_DIR` set).
- **Shot**: an offscreen `--shot` render was inspected by an agent.
- **User**: the user played it in the viewer.
- Blank means untested. "Not ported" items come from each script's module doc.

Campaign number → internal script: 3 is `level6.rs` (Bombs Away), 6 is `level3.rs` (Slime
Time); the rest match. 11–15 load `data\level01`–`05\level1.*`.

## 2 Andy's Neighborhood (`level2.rs`)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| See-saws (2) | `see_saws_lie_along_their_yaw`, `seesaw.rs` | | |
| Sarge's five troops (token 1) | `sarge_wants_his_troops` | | |
| Troop in the lawn holes: stomp six (2026-10-31 port) | `troop_in_the_holes_is_freed_by_stomping_six`, `stomping_away_from_the_hole_does_nothing` | | reported broken before the port |
| Zurg kite on the roof (token 4) | `kite_challenge_on_the_roof` | | |
| Pool water, pool ball and squash pad | `pool_water_slows_falls`, `pool_ball_deflates_then_pumping_launches_it_with_collision` | | |
| Drawbridge and tree pushed over | `pushed_drawbridge_falls_and_swaps_collision`, `tree_only_tips_when_pushed_from_its_upper_platform` | | |
| Two stomp switches, planks, gate, spring | | | |
| Rope on pole 0 | | | |
| Hamm (token 0), hint Hamm | | | |
| Background picture matches sky fill | | 2026-10-31 | reported mismatch before fix |
| Not ported: kite string, sky effects, hole dirt model | | | |

## 3 Bombs Away (`level6.rs`, boss)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Fly-in look-at scene at x 0x31F9E | | | |
| 600-tick cycle: hover, suck, drop bombs, fly off | | | |
| Hits, invulnerable scene, death, level end | | | |
| Own music throughout (0x49EAC0 not called) | `music::boss_levels_keep_their_own_track` | | reported switching to miniboss theme before fix |
| Not ported: camera drift | | | |

## 4 Construction Yard (`level4.rs`)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Hanging platforms, path movers | `platforms_follow_their_paths` | | |
| Presses, drawbridge stomp, buttons and lift | | | |
| Timed challenge (token 2), finder (token 1) | | | |
| Drill boss (token 4) | `drill_challenge` | | |
| Paint puzzle (token 3) | `construction_paint_*` (2) | | confirmed |
| Paint-room hatch folds open near Buzz (0x41C640, 0x52F7D0) | `paint_room_hatch_opens_for_buzz` | `cmp_hatch_play.png` | reported textured before fix |
| Interior seen only through portals (0x4BC460, level 4 only) | `paint_room_seen_only_through_its_portals` | `cmp_far*.png`; Level 1 identical | reported before fix; creatures/pickups not room-gated yet |
| Not ported: drips, lights | | | |

## 5 Alleys and Gullies (`level5.rs`)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Gliding platforms | `platforms_glide` | | |
| See-saws (2), trampoline | `continuation_audit` example (see-saws) | | |
| Clown boss (token 4) | `clown_challenge_in_room_2` | | |
| Bones challenge (2): all five exist, show and can be collected | `bones_all_collectable` | `l5-bones-0.png` | user's report was really the missing countdown |
| Challenge/race countdown (HUD element 6 ≥ 100, 0x49FD40) | `hud::laps_from_100_are_a_countdown` | `l5-countdown.png` | reported missing; fixed |
| Finder ducks (1), Hamm, hint | | | |
| Not ported: stomp machine and bubbles, falling gate, drips, underwater fog | | | |

## 6 Slime Time (`level3.rs`, boss)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Meeting, four rounds, pickup, death, end | `slime_shrinks_over_four_rounds` | | |
| Laser power-up (special 0x32) from the third round | `slime_shrinks_over_four_rounds` (pickup shown) | | asked; confirmed present |
| Trash-can lid on its head, dropped at death | same test (lid tracks node 10) | 2026-10-31 | reported missing before fix |
| Own music throughout | `music::boss_levels_keep_their_own_track` | | reported before fix |
| Not ported: light and fog, drips, camera shake | | | |

## 7 Al's Toy Barn (`level7.rs`)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Path movers and locks shot | `paths_run` | | |
| Spring, trampoline | | | |
| Skateboards (movers 10 / 0xE): landing on one rolls it, throws Buzz at the end | `skateboard_one_rolls_when_landed_on`, `skateboard_two_rolls_and_returns` | | reported not working; ported, not yet reviewed in a shot |
| Finder chicks (1), egg challenge (2), Potato Head boots, Hamm, hint | | | |
| Dino boss (token 4) | `dino_fight` | | |
| Not ported: bobbing platform race, rockets, chicks pushed by boxes, drips | | | |

## 8 Al's Space Land (`level8.rs`)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Spinning specials | | | |
| Race (token 2) | `race_starts` | | |
| Claw machine (token 3) | `space_land_claw_catches_the_prize_and_drops_it_out` | | |
| Finder aliens (1), Hamm, hint | | | |
| Boss (token 4) | `boss_fight` | | |
| Not ported: electric arcs, room 2 cannon, swaying specials | | | |

## 9 Toy Barn Encounter (`level9.rs`, boss)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Intro, flails, minion rounds, beam, death | `boss_rounds_end_the_level` | | |

## 10 Elevator Hop (`level10.rs`)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Air shafts | `air_shaft_lifts_buzz` | | |
| Wind, lifts, stomp markers | | | |
| Runaway token (2) | `runaway_token` | | |
| Spider (4) | `spider_fight` | | |
| Mice finder (1), Potato Head hook, Hamm, hint | | | |
| Not ported: lifts pausing, lights, electric beams | | | |

## 11 Al's Penthouse (`level11.rs`)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Valves, water, plug | `penthouse_valves_raise_the_water_and_the_plug_drains_it` | | |
| Pipes and boat | `penthouse_pipe_switch_moves_flow_and_the_boat_sails` | | |
| Turrets | `penthouse_turret_button_looks_then_wrecks_its_turret` | | |
| Gunslinger (token 4) | `penthouse_gunslinger_intro_fight_and_token` | | |
| Floats, doors, critter owner (1), horse challenge (2) | | | |

## 12 The Evil Emperor Zurg (`level12.rs`, boss)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Entrance, attacks, recovery, ring, death | `zurg_enters_fires_recovers_and_finishes` | | |
| Entrance stays above the floor (target y) | `zurg_entrance_stays_above_the_floor` | `l12-zurg-intro-*.png` | reported sinking |

## 13 Airport Infiltration (`level13.rs`)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Conveyors, belts, luggage, springs | `airport_platforms_move_and_stomp_springs_launch` | | |
| Conveyor ends hold Buzz (boundary walls, swept push) | `airport_conveyor_ends_hold_buzz_inside_the_level` | `l13-belt-end.png` | reported clipping out |
| Weights challenge | `airport_weights_challenge_times_out_and_can_be_won_on_retry` | | |
| Prospector challenge | `airport_prospector_intro_damage_and_reward` | | |
| Fan wind, characters | | | |

## 14 Tarmac Trouble (`level14.rs`)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Three-press lamp puzzle | `tarmac_switch_sets_its_lamp_and_clears_the_previous_one` | | |
| Rotating plane, propellers, Blacksmith | `tarmac_plane_turns_and_boss_intro_damage_and_reward` | | |
| No damage from far creatures (i32 overflow fix) | `far_creatures_do_not_touch_buzz` | teleport grid 2026-10-31 | reported constant damage before fix |
| Floating token, wing race, luggage | | | |
| Start: no talk or race by itself (0x708 draw distance, not flags) | `standing_at_the_start_starts_no_talk_or_race` | `l14-start-fixed.png` | reported |
| "Things not animated" (user report; what exactly is unknown) | | | reported, open |
| Not ported: rain, lightning, lights | | | |

## 15 Prospector Showdown (`level15.rs`, boss)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Intro, suitcases | `final_arena_lod_suitcases_stay_aligned_during_intro` | suitcases 2026-10-31 | |
| Three bosses, rescue ending | `final_arena_*` (see later_levels.rs) | | |
| Back closed by the boundary wall (kind 0x101) | `final_arena_back_is_closed_by_its_boundary_wall` | `l15-back-wall.png` | reported falling through |
| Swing only while chasing; real hits, defeat, rescue | `final_arena_bosses_swing_only_while_chasing`, `final_arena_fight_hurts_buzz_and_all_three_bosses_fall_to_hits` | | |
| Blacksmith shockwave homing (particle flag 8, 0x410F40) | `particles::flag_8_particles_of_any_kind_home_on_buzz`, floor rules 0x2B / 0x2C | | |

## Front end
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Level select inside the Etch A Sketch (variant 1, 0x494130) | `level_select_flies_to_a_level_and_gates_on_tokens` | 2026-10-31 | reported missing before fix |
| Map cars' heading (0x438910) | | | reported reversed before fix |

## Cross-level
| Item | Status |
|---|---|
| Specials by layer and distance (0x4CDDD0), all levels: no power-ups or helicopter through walls | `specials_of_each_layer_have_their_own_distance`; sweep `out/rv/sweep` |
| Boundary walls (kind 0x101) in every level that has them | `final_arena_back_*`, `airport_conveyor_ends_*` |
| Spawn snap (0x4863A0): only level 13 differed, now starts on the X-ray conveyor | `level_13_starts_on_the_conveyor_above_its_table_point`; `l13-start-orig.png` |
