extends RefCounted

## TST-19 NPC checks: 64 Skyrim units within 30 seconds and changed uploaded bones
## in consecutive one-second pairs. Call sample every frame, then report at a stop.
const METRES_PER_UNIT := 0.0142875
const MOVE_UNITS := 64.0
const WINDOW_MSEC := 30_000
var initial := {}
var poses := {}
var max_units := 0.0
var pairs := 0
var changed_pairs := 0
var walk_frames := 0
var walking := {}
var next_pose_msec := 0
var started_msec := 0

func restart_movement_window(now: int) -> void:
    initial.clear()
    walking.clear()
    max_units = 0.0
    started_msec = now

func sample(player: Node, now: int) -> void:
    if started_msec == 0:
        started_msec = now
    var take_pose := now >= next_pose_msec
    var current_poses := {}
    for visual: Dictionary in player.call("npc_visuals"):
        if visual.node_id == 0 or visual.shapes == 0:
            continue
        var identity := str(visual.file) + ":" + str(visual.object)
        if not initial.has(identity):
            initial[identity] = visual.position
        var actor: Node = instance_from_id(visual.node_id)
        if actor == null or actor.call("render_mode") != "animated":
            continue
        for clip: String in actor.call("active_clips"):
            if clip.to_lower().contains("walk") or clip.to_lower().contains("run"):
                walk_frames += 1
                walking[identity] = true
                break
        if now - started_msec <= WINDOW_MSEC and walking.has(identity):
            max_units = maxf(max_units, visual.position.distance_to(initial[identity]) / METRES_PER_UNIT)
        if take_pose:
            var pose: Array = actor.call("uploaded_skin_poses")
            current_poses[visual.node_id] = pose
            if poses.has(visual.node_id):
                var before: Array = poses[visual.node_id]
                if before.size() != pose.size() or pose.is_empty():
                    continue
                pairs += 1
                for i in pose.size():
                    var a: Transform3D = before[i]
                    var b: Transform3D = pose[i]
                    if not a.is_equal_approx(b):
                        changed_pairs += 1
                        break
    if take_pose:
        poses = current_poses
        next_pose_msec = now + 1000

func report() -> bool:
    var moving := max_units >= MOVE_UNITS and walk_frames > 0
    var animated := pairs > 0 and changed_pairs == pairs
    print("PLAY_PATH npc.moves ", "pass" if moving else "fail", " units=", max_units, " window_s=30 walk_frames=", walk_frames)
    print("PLAY_PATH npc.pose ", "pass" if animated else "fail", " changed=", changed_pairs, " pairs=", pairs, " interval_s=1")
    return moving and animated
