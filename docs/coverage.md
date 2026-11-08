# Campaign interaction coverage (levels 2–15)

The single record of what each level's interactions are and how far each has been checked.
Level 1 is signed off by the user and is the regression reference. Update this table when
an interaction is tested, fixed or reported; HANDOFF.md links here instead of repeating it.

Columns:
- **Sim**: an integration test drives it with real game data (`cargo test --release`, `TS2_GAME_DIR` set).
- **Shot**: an offscreen `--shot` render was inspected by an agent.
- **User**: the user played it in the viewer.
- Blank means untested. "Not ported" items come from each script's module doc.

Every stomp switch in the game (33, levels 1, 2, 4, 7, 8, 10, 11, 14) is landed on with Buzz's
own physics by `tests/stomps.rs` (`every_stomp_switch_registers`): on the impact tick he must
stand on the switch's surface type or touch its mover.

Campaign number → internal script: 3 is `level6.rs` (Bombs Away), 6 is `level3.rs` (Slime
Time); the rest match. 11–15 load `data\level01`–`05\level1.*`.

## 2 Andy's Neighborhood (`level2.rs`)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Conkers fall and splat (spawn modes to 0x1C, kind 0x17, death action 2) | `conkers_fall_and_splat` | | reported floating |
| Leaves falling from the tree (list 0, particle 0x37, 0x4190C0) | `leaves_fall_from_the_tree` | | reported missing |
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
| Intro: banks round its length (creature roll is in its own frame, 0x4CD880 / 0x450C70) | `roll_comes_before_yaw` | intro frames 2026-11-01 | reported tumbling as if hit |
| Fly-in look-at scene at x 0x31F9E | | | |
| 600-tick cycle: hover, suck, drop bombs, fly off | | | |
| Hits, invulnerable scene, death, level end | | | |
| Own music throughout (0x49EAC0 not called) | `music::boss_levels_keep_their_own_track` | | reported switching to miniboss theme before fix |
| Not ported: camera drift | | | |

## 4 Construction Yard (`level4.rs`)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Hanging platforms, path movers: travel and tilt with Buzz on them; only climbing on holds them (0x559E80) | `platforms_follow_their_paths`, `hanging_platforms_travel_with_buzz_on_them` | | reported not moving under him |
| Side lift (mover 0x15): shuttles between the stomped button and the ground (0x41C640) | `side_lift_shuttles_up_to_the_stomped_button` | | reported stuck at the top |
| Presses, drawbridge stomp; a press slamming within 500 shakes the camera by distance (0x41BC20) | `a_press_slamming_near_buzz_shakes_the_camera` | | |
| Lift buttons (movers 0x16-0x18): any contact (0x48B640), tilted -0x180 like their specials | `every_stomp_switch_registers` | | |
| Timed challenge (token 2), finder (token 1) | | | |
| Drill boss (token 4); shakes the camera near Buzz, hurt only while a disc flies (0x41B780) | `drill_challenge` | | |
| Paint puzzle (token 3) | `construction_paint_*` (2) | | confirmed |
| Paint-room hatch folds open near Buzz (0x41C640, 0x52F7D0) | `paint_room_hatch_opens_for_buzz` | `cmp_hatch_play.png` | reported textured before fix |
| Interior seen only through portals (0x4BC460, level 4 only) | `paint_room_seen_only_through_its_portals` | `cmp_far*.png`; Level 1 identical | reported before fix; creatures/pickups not room-gated yet |
| Not ported: pipe flows, the crane's lamps, moving lights | | | |

## 5 Alleys and Gullies (`level5.rs`)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Live zip wires: sparks along list 62 every 0x14 ticks (particle 0x4C) | `sparks_run_along_the_zip_wires` | | reported missing |
| Rain (0x44ED90 sprite 0x34) and splashes | `drops_fall_and_are_replaced` | | reported missing |
| Gliding platforms | `platforms_glide` | | |
| See-saws (2), trampoline | `continuation_audit` example (see-saws) | | |
| Clown boss (token 4) | `clown_challenge_in_room_2` | | |
| Bones challenge (2): all five exist, show and can be collected | `bones_all_collectable` | `l5-bones-0.png` | user's report was really the missing countdown |
| Challenge/race countdown (HUD element 6 ≥ 100, 0x49FD40) | `hud::laps_from_100_are_a_countdown` | `l5-countdown.png` | reported missing; fixed |
| Finder ducks (1), Hamm, hint | | | |
| Rain splashes (0x41E880) | | | |
| Not ported: stomp machine and bubbles, falling gate, underwater fog | | | |

## 6 Slime Time (`level3.rs`, boss)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Meeting, four rounds, pickup, death, end | `slime_shrinks_over_four_rounds` | | |
| Laser power-up (special 0x32) from the third round | `slime_shrinks_over_four_rounds` (pickup shown) | | asked; confirmed present |
| Trash-can lid on its head, dropped at death | same test (lid tracks node 10) | 2026-10-31 | reported missing before fix |
| Own music throughout | `music::boss_levels_keep_their_own_track` | | reported before fix |
| Its timer 3 shakes the camera (0x41AA10) | | | |
| Rain, lightning and its flash (0x41AA10) | | | |
| Not ported: light and fog | | | |

## 7 Al's Toy Barn (`level7.rs`)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Rocket boots: flames on the heels (particles 0x2F, kind 0x15) and state 10 | `rocket_flames_ride_on_the_boots` | | reported no rocket or pose |
| Path movers and locks shot | `paths_run` | | |
| Spring, trampoline | | | |
| Skateboards (movers 10 / 0xE): landing on one rolls it, throws Buzz at the end | `skateboard_one_rolls_when_landed_on`, `skateboard_two_rolls_and_returns` | | reported not working; ported, not yet reviewed in a shot |
| Finder chicks (1), egg challenge (2), Potato Head boots, Hamm, hint | | | |
| Dino boss (token 4); its first breath shakes the camera (0x420AF0) | `dino_fight`, `dino_first_breath_shakes_the_camera` | | |
| "Ride the HayBale" (button mover 0xF, bale mover 0, 0x421340): 45 s countdown, bob, throw | `hay_bale_ride`, `every_stomp_switch_registers` | | reported stomp not working: was not ported |
| Drips (0x421340) | | | |
| Not ported: rockets, chicks pushed by boxes | | | |

## 8 Al's Space Land (`level8.rs`)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Spinning specials | | | |
| Race (token 2) | `race_starts` | | |
| Claw machine (token 3) | `space_land_claw_catches_the_prize_and_drops_it_out` | | |
| Finder aliens (1), Hamm, hint | | | |
| Boss (token 4) | `boss_fight` | | |
| Electric arcs (0x422D20), room 2's lobbers, room 4's swaying specials | `arcs_strike_buzz_in_their_area`, `room_2_lobbers_throw_at_buzz`, `room_4_specials_sway` | | |

## 9 Toy Barn Encounter (`level9.rs`, boss)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Scene camera keeps looking at the burst pod (dead flail's last tip) | `minions_come_out_at_the_flail_tip` | | reported camera inside the boss |
| Intro, flails, minion rounds, beam, death | `boss_rounds_end_the_level` | | |
| Flail tips on animated node 1 (0x43C1C0), minions called up there; flails half a cycle apart; call animation and sparks | `minions_come_out_at_the_flail_tip` | | user: rest of the call-up looks good; "clips inside the boss for a second" after it, not reproduced |
| Beam from the eye (node 0, (0, 0, -400), 0x43C1C0) | | | reported coming from the bottom |

## 10 Elevator Hop (`level10.rs`)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Grappling hook: lock (X) survives mouse jitter, throws and pulls; every level 10 climb (hook points on screen, on their floors) | `every_hook_climb_on_level_10_reaches_the_next_ledge`, `hook_points_are_drawn` | | reported: hooks invisible, stuck after the first |
| Air shafts | `air_shaft_lifts_buzz` | | |
| Wind, lifts, stomp markers | | | |
| Vents 4 / 5 stomped open (any contact, 0x48B640) | `every_stomp_switch_registers` | | |
| Runaway token (2) | `runaway_token` | | |
| Spider (4) | `spider_fight` | | |
| Mice finder (1), Potato Head hook, Hamm, hint | | | |
| Lifts: the follow camera while riding (objects flagged 0x200 ignored, 0x48C860) | `the_camera_rides_a_lift_smoothly` | | reported juddering (as level 11) |
| Wall mice (0x425A80, slots 1-5): height and depth swapped about home | `wall_mice_crawl_up_their_walls` | `mouse.png` 2026-11-08 | reported clipping through walls |
| Not ported: lights | | | |

## 11 Al's Penthouse (`level11.rs`)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Valves, water, plug | `penthouse_valves_raise_the_water_and_the_plug_drains_it` | | |
| Pipes and boat | `penthouse_pipe_switch_moves_flow_and_the_boat_sails` | | |
| Turrets | `penthouse_turret_button_looks_then_wrecks_its_turret` | | |
| Gunslinger (token 4) | `penthouse_gunslinger_intro_fight_and_token` | | |
| Side shooters (0x2E): the shot's life ends at the first wall (0x406A90 / 0x48C860) | `side_shooter_shot_lives_until_the_first_wall` | | |
| Floats, doors, critter owner (1), horse challenge (2) | | | |
| Far-set copies (0x50, 0x57, 0x40) follow in level units (were placed at a quarter) | | | |

## 12 The Evil Emperor Zurg (`level12.rs`, boss)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Entrance, attacks, recovery, ring, death | `zurg_enters_fires_recovers_and_finishes` | | |
| Entrance stays above the floor (target y) | `zurg_entrance_stays_above_the_floor` | `l12-zurg-intro-*.png` | reported sinking |
| Entrance jittery: smooth tick to tick; the user's display is 144 Hz, so the viewer now draws between ticks (`Interp`), and snaps frame times on 60 Hz | `frame_times_snap_to_ticks` | consecutive frames compared 2026-11-01 | reported twice; interpolation unconfirmed |
| Chase keeps 2000 from Buzz (0x4076F0 type 0x39), higher while retaliating | `zurg_keeps_his_distance_while_chasing` | | reported following Buzz constantly |

## 13 Airport Infiltration (`level13.rs`)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Oil (surface type 5): little grip (0x436220 types 4-7) | `oil_is_slippery` | | reported not slippery |
| Oil: splash on it (0x1D), prints (0x21) on ordinary floor after it (0x4A2D80) | | `oil6.png` with `oily`, 2026-11-08 | reported no prints; needs the spot |
| Conveyors, belts, luggage, springs | `airport_platforms_move_and_stomp_springs_launch` | | |
| Conveyor ends hold Buzz (boundary walls, swept push) | `airport_conveyor_ends_hold_buzz_inside_the_level` | `l13-belt-end.png` | reported clipping out |
| Weights challenge | `airport_weights_challenge_times_out_and_can_be_won_on_retry` | | |
| Prospector challenge | `airport_prospector_intro_damage_and_reward` | | |
| Fan wind, characters | | | |

## 14 Tarmac Trouble (`level14.rs`)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Rain (0x44ED90 sprite 0x33) and splashes | `drops_fall_and_are_replaced` | rain14.png 2026-11-01 | reported missing |
| Three-press lamp puzzle | `tarmac_switch_sets_its_lamp_and_clears_the_previous_one`, `stomping_a_helicopter_button_presses_it` | button stomp 2026-11-01 | reported "stomps not working": not reproduced |
| Rotating plane, propellers, Blacksmith | `tarmac_plane_turns_and_boss_intro_damage_and_reward` | | |
| No damage from far creatures (i32 overflow fix) | `far_creatures_do_not_touch_buzz` | teleport grid 2026-10-31 | reported constant damage before fix |
| Floating token, wing race, luggage | | | |
| Start: no talk or race by itself (0x708 draw distance, not flags) | `standing_at_the_start_starts_no_talk_or_race` | `l14-start-fixed.png` | reported |
| "Things not animated" (user report; what exactly is unknown) | | | reported, open |
| Lightning: flash, screen wash (0x44DD80) and thunder (0x42E790) | `lightning_flashes_and_thunder_follows`, `the_white_wash_follows_the_colour` | `l14_c.png` 2026-11-08 | reported no flash |
| Not ported: lights | | | |

## 15 Prospector Showdown (`level15.rs`, boss)
| Interaction | Sim | Shot | User |
|---|---|---|---|
| Sliding slope (floor type 0xD, 0x53C818) | `sliding_floors_carry_buzz_downhill` | | |
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
| Moving platforms carry Buzz however they're moved (levels 4, 7, 10: every flat-topped mover, rising in and between ticks) | `rising_platforms_carry_buzz`; user reported falling through the level 4 claw and level 10 lifts |
| A platform rising into a jumping or falling Buzz puts him on it (levels 2, 4, 7, 10); planks tipping up lift him; level 4's platforms and shuttles under the real script | `a_rising_platform_meeting_buzz_puts_him_on_it`, `a_tipping_plank_lifts_buzz`, `a_mover_coming_sideways_pushes_buzz`, `buzz_stays_on_the_moving_platforms`, `rocket_double_jump_spreads_the_wings`; user reported the level 2 float, level 4 claw and iron bar |
| Level tester: every ability given, gadget cages open | |
| Camera shake (0x50A510): the follow camera's aim pitch wobbles (0x4045E0); set by stomps, level 4's presses and drill, the dino, the slime | `shake_wobbles_the_aim_and_dies_away`, `stomp_hangs_then_slams`; not seen in a shot; first person doesn't wobble |
| Countdown tick (sound 0x5E, owned loop) while a challenge countdown shows (0x49FD40) | `countdown_ticks_only_while_shown`; not heard by the agent |
| Disc launcher (ammo B): fire throws discs, homing (0x47) or straight (0x48), at most 6; the HUD's ammo count | `discs_replace_the_laser`, `homing_discs_hit_or_bounce`, `disc_count_shows_on_the_weapon_meter`; not played yet |
| Specials by layer and distance (0x4CDDD0), all levels: no power-ups or helicopter through walls | `specials_of_each_layer_have_their_own_distance`; sweep `out/rv/sweep` |
| Boundary walls (kind 0x101) in every level that has them | `final_arena_back_*`, `airport_conveyor_ends_*` |
| Spawn snap (0x4863A0): only level 13 differed, now starts on the X-ray conveyor | `level_13_starts_on_the_conveyor_above_its_table_point`; `l13-start-orig.png` |

## Added 2026-11-08 (not played)

| Feature | Test | Shot | Report |
|---|---|---|---|
| Floor shadows (0x445720) | `shadows::tests` | `shadow3c.png` | |
| Lens flares (0x44F580) | `flares::tests` | | |
| Pause menu (0x49F4B0) | `pause::tests` | `pause.png` | |
| Options, movie viewer, credits, loading pictures, attract demo | `frontend::tests` | | |
| Level 8's boxes (0x4F2F44), also levels 5 and 7 | `level_8_boxes_are_loaded_and_push` | | reported unpushable |
