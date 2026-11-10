use eldenring::cs::{CSTaskGroupIndex, CSTaskImp, UserInputKey};
use eldenring::dluid::DLUserInputDeviceImpl;
use eldenring::fd4::{FD4PadManager, FD4TaskData, InputType};
use fromsoftware_shared::{FromStatic, RecurringTaskHandle, SharedTaskImpExt, Subclass};
use std::sync::Mutex;

/// Keys being held and for how many more frames.
static HELD: Mutex<Vec<(UserInputKey, u32)>> = Mutex::new(Vec::new());

pub fn key(name: &str) -> Option<UserInputKey> {
    Some(match name {
        "attack" => UserInputKey::Attack,
        "strong" => UserInputKey::StrongAttack,
        "guard" => UserInputKey::Guard,
        "skill" => UserInputKey::Skill,
        "roll" => UserInputKey::BackstepTapped,
        "jump" => UserInputKey::Jump,
        "item" => UserInputKey::UseItem,
        "lockon" => UserInputKey::ResetCameraTapped,
        "action" => UserInputKey::EventAction,
        _ => return None,
    })
}

pub fn hold(key: UserInputKey, frames: u32) {
    HELD.lock().unwrap().push((key, frames));
}

/// What a probe saw of one key: in how many frames it read as down in the
/// live input and in the staged copy.
#[derive(Clone, Copy)]
struct Probe {
    key: UserInputKey,
    left: u32,
    seen: u32,
    live: u32,
    staged: u32,
}

static PROBE: Mutex<Option<Probe>> = Mutex::new(None);

/// Watches `key` for `frames` frames, to tell whether this task sees real
/// presses at all: hold the key in the game while probing.
pub fn probe(key: UserInputKey, frames: u32) {
    *PROBE.lock().unwrap() = Some(Probe {
        key,
        left: frames,
        seen: 0,
        live: 0,
        staged: 0,
    });
}

pub fn probe_status() -> Option<String> {
    let p = (*PROBE.lock().unwrap())?;
    let idx = indices(p.key)
        .map(|v| format!("{v:?}"))
        .unwrap_or_else(|e| e.into());
    Some(format!(
        "{:?} left={} frames={} live={} staged={} indices={idx}",
        p.key, p.left, p.seen, p.live, p.staged
    ))
}

/// Writes held keys into the virtual input the in-game pad reads. It runs in
/// GameFlowStep, the group right after PadStep, aiming to land after the
/// devices are refreshed for the frame and before characters read input.
pub fn install(task: &CSTaskImp) -> RecurringTaskHandle<FD4TaskData> {
    task.run_recurring(
        |_: &FD4TaskData| {
            if let Some(p) = PROBE.lock().unwrap().as_mut()
                && p.left > 0
            {
                let (live, staged) = read(p.key).unwrap_or((false, false));
                p.left -= 1;
                p.seen += 1;
                p.live += live as u32;
                p.staged += staged as u32;
            }
            let mut held = HELD.lock().unwrap();
            for (key, _) in held.iter() {
                let _ = press(*key);
            }
            held.retain_mut(|(_, n)| {
                *n -= 1;
                *n > 0
            });
        },
        CSTaskGroupIndex::GameFlowStep,
    )
}

fn device() -> Result<&'static mut DLUserInputDeviceImpl, &'static str> {
    let man = unsafe { FD4PadManager::instance_mut() }.map_err(|_| "no pad manager")?;
    let pad = man.get_in_game_pad_mut().ok_or("no in-game pad")?;
    Ok(unsafe { pad.pad_device.as_mut().virtual_multi_device.as_mut() }.superclass_mut())
}

/// Virtual input indices the in-game pad reads for `key` as held down.
fn indices(key: UserInputKey) -> Result<Vec<usize>, &'static str> {
    let man = unsafe { FD4PadManager::instance() }.map_err(|_| "no pad manager")?;
    let pad = man.get_in_game_pad().ok_or("no in-game pad")?;
    let group = unsafe { pad.input_type_group.as_ref() }
        .find(&key)
        .ok_or("key not mapped")?;
    let key_assign = unsafe { pad.key_assign.as_ref() };
    Ok(group
        .iter()
        .filter(|(_, t)| matches!(t, InputType::AreKeysDown))
        .filter_map(|(m, _)| key_assign.get_virtual_input_index(m))
        .map(|i| i as usize)
        .collect())
}

fn read(key: UserInputKey) -> Result<(bool, bool), &'static str> {
    let d = device()?;
    let idx = indices(key)?;
    let live = idx.iter().any(|&i| d.virtual_input_data.get_digital(i));
    let staged = idx
        .iter()
        .any(|&i| d.initial_virtual_input_data.get_digital(i));
    Ok((live, staged))
}

/// Sets the key in both the live input and the staged copy the device copies
/// over it, since which of the two survives until the pad reads is not known.
/// The pad is also made to poll: presses only took effect while the game
/// window had focus, and `allow_polling` is the pad's own on/off switch.
fn press(key: UserInputKey) -> Result<(), &'static str> {
    let idx = indices(key)?;
    let man = unsafe { FD4PadManager::instance_mut() }.map_err(|_| "no pad manager")?;
    man.get_in_game_pad_mut()
        .ok_or("no in-game pad")?
        .allow_polling = true;
    let d = device()?;
    for i in idx {
        d.virtual_input_data.set_digital(i, true);
        d.initial_virtual_input_data.set_digital(i, true);
    }
    Ok(())
}

/// Whether the in-game pad is polling input at all right now.
pub fn polling() -> Option<bool> {
    let man = unsafe { FD4PadManager::instance() }.ok()?;
    Some(man.get_in_game_pad()?.allow_polling)
}
