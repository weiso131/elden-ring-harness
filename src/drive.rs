use crate::input::{Target, hold};
use eldenring::cs::{CSCamExt, CSCamera, ChrIns, UserInputKey, WorldChrMan};
use eldenring::rotation::Quaternion;
use fromsoftware_shared::FromStatic;
use std::ptr::NonNull;
use std::sync::Mutex;

/// A lock-on in progress, optionally setting the locked enemy's HP the frame
/// the lock appears.
struct Lock {
    hp: Option<u32>,
    tries: u32,
    phase: Phase,
    /// Frames left in this phase.
    wait: u32,
    result: String,
}

#[derive(PartialEq)]
enum Phase {
    Idle,
    /// Turned to the target, letting the turn settle before pressing.
    Facing,
    /// Pressed, watching for the lock.
    Pressed,
}

static LOCK: Mutex<Lock> = Mutex::new(Lock {
    hp: None,
    tries: 0,
    phase: Phase::Idle,
    wait: 0,
    result: String::new(),
});

const SETTLE: u32 = 10;
const LOCK_WAIT: u32 = 30;

pub fn lock(hp: Option<u32>) {
    *LOCK.lock().unwrap() = Lock {
        hp,
        tries: 3,
        phase: Phase::Idle,
        wait: 0,
        result: "waiting".into(),
    };
}

/// A walk to a point, steering every frame.
struct Walk {
    to: (f32, f32),
    radius: f32,
    left: u32,
    result: String,
}

static WALK: Mutex<Walk> = Mutex::new(Walk {
    to: (0.0, 0.0),
    radius: 0.0,
    left: 0,
    result: String::new(),
});

pub fn walk(x: f32, z: f32, radius: f32, frames: u32) {
    *WALK.lock().unwrap() = Walk {
        to: (x, z),
        radius,
        left: frames,
        result: "walking".into(),
    };
}

pub fn stop() {
    let mut w = WALK.lock().unwrap();
    if w.left > 0 {
        w.left = 0;
        w.result = "stopped".into();
    }
    let mut l = LOCK.lock().unwrap();
    if l.tries > 0 || l.phase != Phase::Idle {
        l.tries = 0;
        l.phase = Phase::Idle;
        l.result = "stopped".into();
    }
}

/// Turns the player to look at `t` along the ground.
pub fn face(w: &mut WorldChrMan, t: NonNull<ChrIns>) -> Result<f32, &'static str> {
    let p = w.main_player.as_mut().ok_or("no player")?;
    let tp = &unsafe { t.as_ref() }.modules.physics.position;
    let physics = &mut p.chr_ins.modules.physics;
    let (dx, dz) = (tp.0 - physics.position.0, tp.2 - physics.position.2);
    // Assumes a character's forward is its local +Z, yaw turning it towards
    // +X; not checked in the game yet.
    let yaw = dx.atan2(dz);
    let q = Quaternion(0.0, (yaw / 2.0).sin(), 0.0, (yaw / 2.0).cos());
    physics.orientation = q;
    physics.interpolated_orientation = q;
    Ok(yaw.to_degrees())
}

/// Holds the movement stick for `frames` at `angle` degrees clockwise from
/// where the camera looks, pushed all the way: movement follows the camera,
/// not the character.
pub fn push(angle: f32, frames: u32) {
    let (s, c) = angle.to_radians().sin_cos();
    for (key, v) in [
        (UserInputKey::MoveForwards, c),
        (UserInputKey::MoveBackwards, -c),
        (UserInputKey::MoveRight, s),
        (UserInputKey::MoveLeft, -s),
    ] {
        if v > 0.01 {
            hold(Target::Stick(key, v), frames);
        }
    }
}

/// The ground direction from the camera's forward to world direction (dx, dz),
/// in degrees clockwise.
fn camera_angle(dx: f32, dz: f32) -> Option<f32> {
    let cam = unsafe { CSCamera::instance() }.ok()?;
    let f = cam.pers_cam_1.forward();
    let r = cam.pers_cam_1.right();
    let (fwd, right) = (dx * f.0 + dz * f.2, dx * r.0 + dz * r.2);
    Some(right.atan2(fwd).to_degrees())
}

/// Runs once a frame on the main thread, before input is applied.
pub fn step() {
    lock_step();
    walk_step();
}

fn lock_step() {
    let mut l = LOCK.lock().unwrap();
    if l.phase == Phase::Idle && l.tries == 0 {
        return;
    }
    let Ok(w) = (unsafe { WorldChrMan::instance_mut() }) else {
        return;
    };
    let Some(handle) = w.main_player.as_ref().map(|p| p.locked_on_enemy) else {
        return;
    };
    if l.phase == Phase::Pressed
        && let Some(t) = w.chr_ins_by_handle_mut(&handle)
    {
        let data = &mut t.modules.data;
        l.result = match l.hp {
            Some(pct) => {
                data.hp = (data.max_hp as i64 * pct as i64 / 100) as i32;
                format!("done hp={}/{}", data.hp, data.max_hp)
            }
            None => format!("done max_hp={}", data.max_hp),
        };
        l.tries = 0;
        l.phase = Phase::Idle;
        return;
    }
    if l.wait > 0 {
        l.wait -= 1;
        return;
    }
    match l.phase {
        // A press with nothing in view only turns the camera behind the
        // player, who faces the target by now, so a retry can lock.
        Phase::Idle | Phase::Pressed if l.tries == 0 => {
            l.phase = Phase::Idle;
            l.result = "no lock".into();
        }
        Phase::Idle | Phase::Pressed => {
            let Some(t) = crate::control::target(w) else {
                l.tries = 0;
                l.phase = Phase::Idle;
                l.result = "no target".into();
                return;
            };
            let _ = face(w, t);
            l.phase = Phase::Facing;
            l.wait = SETTLE;
            l.result = "facing".into();
        }
        Phase::Facing => {
            // One press a try: a second one while locked would drop the lock.
            hold(Target::Key(UserInputKey::ResetCameraTapped), 2);
            l.tries -= 1;
            l.phase = Phase::Pressed;
            l.wait = LOCK_WAIT;
            l.result = "pressed".into();
        }
    }
}

fn walk_step() {
    let mut walk = WALK.lock().unwrap();
    if walk.left == 0 {
        return;
    }
    walk.left -= 1;
    let Some(p) = (unsafe { WorldChrMan::instance() })
        .ok()
        .and_then(|w| w.main_player.as_ref())
    else {
        return;
    };
    let me = &p.chr_ins.modules.physics.position;
    let (dx, dz) = (walk.to.0 - me.0, walk.to.1 - me.2);
    let dist = (dx * dx + dz * dz).sqrt();
    if dist < walk.radius {
        walk.left = 0;
        walk.result = format!("done dist={dist:.1}");
        return;
    }
    if walk.left == 0 {
        walk.result = format!("timeout dist={dist:.1}");
        return;
    }
    if let Some(a) = camera_angle(dx, dz) {
        push(a, 1);
    }
    walk.result = format!("walking dist={dist:.1}");
}

pub fn status(s: &mut String) {
    use std::fmt::Write as _;
    let l = &LOCK.lock().unwrap().result;
    if !l.is_empty() {
        let _ = writeln!(s, "lockon\t{l}");
    }
    let w = &WALK.lock().unwrap().result;
    if !w.is_empty() {
        let _ = writeln!(s, "walk\t{w}");
    }
}
