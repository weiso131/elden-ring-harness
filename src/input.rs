use eldenring::cs::{CSTaskGroupIndex, CSTaskImp, UserInputKey};
use eldenring::dluid::{DLUserInputDeviceImpl, DLUserInputManagerImpl};
use eldenring::fd4::{FD4PadManager, FD4TaskData, InputType};
use fromsoftware_shared::{FromStatic, RecurringTaskHandle, SharedTaskImpExt, Subclass};
use std::sync::Mutex;

/// What a press holds down: a game action, mapped through the in-game pad's
/// bindings, or a raw virtual input index, which any pad reads, menus included.
#[derive(Clone, Copy)]
pub enum Target {
    Key(UserInputKey),
    Raw(usize),
}

/// Targets being held and for how many more frames.
static HELD: Mutex<Vec<(Target, u32)>> = Mutex::new(Vec::new());

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

pub fn hold(target: Target, frames: u32) {
    HELD.lock().unwrap().push((target, frames));
}

/// Frames left to scan, and for each raw index seen down, in how many frames.
type Scan = (u32, std::collections::BTreeMap<usize, u32>);
static SCAN: Mutex<Option<Scan>> = Mutex::new(None);

/// Records which raw indices are down over `frames` frames: press a key in the
/// game while scanning to learn its index, e.g. a menu's confirm key.
pub fn scan(frames: u32) {
    *SCAN.lock().unwrap() = Some((frames, Default::default()));
}

pub fn scan_status() -> Option<String> {
    let s = SCAN.lock().unwrap();
    let (left, seen) = s.as_ref()?;
    Some(format!("left={left} down={seen:?}"))
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
            if let Some((left, seen)) = SCAN.lock().unwrap().as_mut()
                && *left > 0
                && let Ok(d) = device()
            {
                *left -= 1;
                // integer_count counts bytes (bits / 32 * 4, per the library),
                // so reading that many u32s runs past the bitset.
                let bits = d.virtual_input_data.dynamic_bitset.integer_count * 8;
                for i in 0..bits {
                    if d.virtual_input_data.get_digital(i) {
                        *seen.entry(i).or_default() += 1;
                    }
                }
            }
            let mut held = HELD.lock().unwrap();
            focus(!held.is_empty());
            for (target, _) in held.iter() {
                let _ = press(*target);
            }
            held.retain_mut(|(_, n)| {
                *n -= 1;
                *n > 0
            });
        },
        CSTaskGroupIndex::GameFlowStep,
    )
}

/// The focus state the game had before presses started, while they last.
static SAVED_FOCUS: Mutex<Option<bool>> = Mutex::new(None);

/// The game stops taking input while its window is not focused, and a script
/// runs from a terminal; while keys are held the game is told it has focus,
/// and afterwards it gets back what it had, so typing in the terminal does not
/// reach it once the presses are over.
fn focus(pressing: bool) {
    let Ok(man) = (unsafe { DLUserInputManagerImpl::instance_mut() }) else {
        return;
    };
    let mut saved = SAVED_FOCUS.lock().unwrap();
    match (pressing, *saved) {
        (true, None) => {
            *saved = Some(man.is_game_window_focused);
            man.is_game_window_focused = true;
        }
        (true, Some(_)) => man.is_game_window_focused = true,
        (false, Some(was)) => {
            man.is_game_window_focused = was;
            *saved = None;
        }
        (false, None) => {}
    }
}

pub fn focused() -> Option<bool> {
    Some(
        unsafe { DLUserInputManagerImpl::instance() }
            .ok()?
            .is_game_window_focused,
    )
}

/// The device the pads read. On the title screen there may be no in-game pad
/// yet, so the menu pad's device is the fallback.
fn device() -> Result<&'static mut DLUserInputDeviceImpl, &'static str> {
    let man = unsafe { FD4PadManager::instance_mut() }.map_err(|_| "no pad manager")?;
    let mut pad_device = match man.get_in_game_pad_mut() {
        Some(p) => p.pad_device,
        None => man.get_menu_viewer_pad_mut().ok_or("no pad")?.pad_device,
    };
    Ok(unsafe { pad_device.as_mut().virtual_multi_device.as_mut() }.superclass_mut())
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
fn press(target: Target) -> Result<(), &'static str> {
    let idx = match target {
        Target::Key(key) => indices(key)?,
        Target::Raw(i) => vec![i],
    };
    let man = unsafe { FD4PadManager::instance_mut() }.map_err(|_| "no pad manager")?;
    if let Some(pad) = man.get_in_game_pad_mut() {
        pad.allow_polling = true;
    }
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
