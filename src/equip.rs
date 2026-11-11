use eldenring::cs::{ChrAsm, GaitemHandle, WorldChrMan};
use fromsoftware_shared::FromStatic;
use std::fmt::Write as _;

fn raw(h: GaitemHandle) -> u32 {
    unsafe { std::mem::transmute(h) }
}

/// The six weapon slots: left 1, right 1, left 2, right 2, left 3, right 3.
const WEAPON_SLOTS: usize = 6;

/// Lists the weapon slots as the player's game data and the character each
/// hold them, to compare before and after equipping from the menu.
pub fn dump() -> Result<String, &'static str> {
    let w = unsafe { WorldChrMan::instance() }.map_err(|_| "no WorldChrMan")?;
    let p = w.main_player.as_ref().ok_or("no player")?;
    let eq = &unsafe { p.player_game_data.as_ref() }.equipment;
    let mut out = format!(
        "ok arm_style={:?} right_slot={} left_slot={}\nslot\tidx\tgame_handle\tgame_param\tchr_handle\tchr_param\n",
        eq.chr_asm.equipment.arm_style,
        eq.chr_asm.equipment.selected_slots.right_weapon_slot,
        eq.chr_asm.equipment.selected_slots.left_weapon_slot,
    );
    let chr: &ChrAsm = &p.chr_asm;
    for i in 0..WEAPON_SLOTS {
        let _ = writeln!(
            out,
            "{i}\t{}\t0x{:08x}\t{}\t0x{:08x}\t{}",
            eq.equipment_item_idx_list[i],
            raw(eq.chr_asm.gaitem_handles[i]),
            eq.chr_asm.equipment_param_ids[i],
            raw(chr.gaitem_handles[i]),
            chr.equipment_param_ids[i],
        );
    }
    Ok(out)
}

/// Puts the inventory's weapon `id` in weapon slot `slot` (as `dump` numbers
/// them) by writing the slot directly, in both the game data and the
/// character, the way the equipment menu is assumed to.
pub fn equip(id: u32, slot: usize) -> Result<String, String> {
    if slot >= WEAPON_SLOTS {
        return Err(format!("not a weapon slot: {slot}"));
    }
    let w = unsafe { WorldChrMan::instance_mut() }.map_err(|_| "no WorldChrMan")?;
    let p = w.main_player.as_mut().ok_or("no player")?;
    let eq = &mut unsafe { p.player_game_data.as_mut() }.equipment;
    let (idx, handle) = eq
        .equip_inventory_data
        .items_data
        .normal_entries()
        .iter()
        .enumerate()
        .find_map(|(i, e)| {
            let e = e.as_option()?;
            (e.item_id.into_inner() == id).then_some((i, e.gaitem_handle))
        })
        .ok_or_else(|| format!("no item 0x{id:08x} in the inventory"))?;
    let before = (
        eq.equipment_item_idx_list[slot],
        raw(eq.chr_asm.gaitem_handles[slot]),
        eq.chr_asm.equipment_param_ids[slot],
    );
    eq.equipment_item_idx_list[slot] = idx as u32;
    eq.chr_asm.gaitem_handles[slot] = handle;
    eq.chr_asm.equipment_param_ids[slot] = id as i32;
    p.chr_asm.gaitem_handles[slot] = handle;
    p.chr_asm.equipment_param_ids[slot] = id as i32;
    Ok(format!(
        "slot={slot} idx={idx} handle=0x{:08x}; was idx={} handle=0x{:08x} param={}",
        raw(handle),
        before.0,
        before.1,
        before.2
    ))
}
