use eldenring::cs::{CSTaskGroupIndex, CSTaskImp, UserInputKey};
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

/// Writes held keys into the virtual input the in-game pad reads. It runs in
/// GameFlowStep, the group right after PadStep, aiming to land after the
/// devices are refreshed for the frame and before characters read input.
pub fn install(task: &CSTaskImp) -> RecurringTaskHandle<FD4TaskData> {
    task.run_recurring(
        |_: &FD4TaskData| {
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

fn press(key: UserInputKey) -> Result<(), &'static str> {
    let man = unsafe { FD4PadManager::instance_mut() }.map_err(|_| "no pad manager")?;
    let pad = man.get_in_game_pad_mut().ok_or("no in-game pad")?;
    let group = unsafe { pad.input_type_group.as_ref() }
        .find(&key)
        .ok_or("key not mapped")?;
    let key_assign = unsafe { pad.key_assign.as_ref() };
    let indices: Vec<i32> = group
        .iter()
        .filter(|(_, t)| matches!(t, InputType::AreKeysDown))
        .filter_map(|(m, _)| key_assign.get_virtual_input_index(m))
        .collect();
    let device = unsafe { pad.pad_device.as_mut().virtual_multi_device.as_mut() };
    for i in indices {
        device
            .superclass_mut()
            .set_virtual_digital_state(i as usize, true);
    }
    Ok(())
}
