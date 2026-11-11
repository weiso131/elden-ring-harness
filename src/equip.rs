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
    let items = &eq.equip_inventory_data.items_data;
    let (i, handle) = items
        .normal_entries()
        .iter()
        .enumerate()
        .find_map(|(i, e)| {
            let e = e.as_option()?;
            (e.item_id.into_inner() == id).then_some((i, e.gaitem_handle))
        })
        .ok_or_else(|| format!("no item 0x{id:08x} in the inventory"))?;
    // The menu numbers items across both lists, key items first: a weapon
    // equipped from it read as its normal-list position plus this.
    let idx = i + items.key_items_capacity as usize;
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

/// Writes every item, key items included, in the inventory and the storage
/// box to `path`, and answers with how full each is.
pub fn inventory(path: &std::path::Path) -> Result<String, String> {
    let w = unsafe { WorldChrMan::instance() }.map_err(|_| "no WorldChrMan")?;
    let p = w.main_player.as_ref().ok_or("no player")?;
    let data = unsafe { p.player_game_data.as_ref() };
    let mut out = String::from("where\tlist\tindex\tid\thandle\tquantity\n");
    let mut summary = String::from("ok");
    let lists = [
        ("inventory", Some(&data.equipment.equip_inventory_data)),
        ("storage", data.storage.as_deref()),
    ];
    for (name, inv) in lists {
        let Some(inv) = inv else { continue };
        let items = &inv.items_data;
        let _ = write!(
            summary,
            " {name}: normal {}/{} key {}/{}",
            items.normal_items_len,
            items.normal_items_capacity,
            items.key_items_len,
            items.key_items_capacity
        );
        // The storage box keeps no key items, and its key list may not be
        // allocated.
        let key: &[_] = if items.key_items_len > 0 {
            items.current_key_entries()
        } else {
            &[]
        };
        for (list, entries) in [("key", key), ("normal", items.normal_entries())] {
            for (i, e) in entries.iter().enumerate() {
                if let Some(e) = e.as_option() {
                    let _ = writeln!(
                        out,
                        "{name}\t{list}\t{i}\t0x{:08x}\t0x{:08x}\t{}",
                        e.item_id.into_inner(),
                        raw(e.gaitem_handle),
                        e.quantity
                    );
                }
            }
        }
    }
    std::fs::write(path, out).map_err(|e| e.to_string())?;
    Ok(summary)
}
