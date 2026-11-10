use eldenring::cs::{BonfireWarpParam, CSLuaEventManImp, SoloParamRepository};
use fromsoftware_shared::FromStatic;
use fromsoftware_shared::program::Program;
use pelite::pattern;
use pelite::pe64::Pe;
use std::sync::LazyLock;

/// The game's own warp-to-grace, as called by the grace menu. Not bound by
/// fromsoftware-rs; the pattern and the arguments are the ones Erd-Tools
/// (Nordgaren) uses: the pattern starts at the `ret` ending the previous
/// function, and the function starts two bytes later.
const WARP_PATTERN: &str = "C3 ? ? ? ? ? ? 57 48 83 EC ? 48 8B FA 44";

type WarpFn = extern "C" fn(script_imitation: usize, proxy: usize, bonfire_entity_id: u32);

static WARP: LazyLock<Result<WarpFn, String>> = LazyLock::new(|| {
    let program = Program::current();
    let pat = pattern::parse(WARP_PATTERN).map_err(|e| format!("bad pattern: {e}"))?;
    let mut matches = program.scanner().matches_code(&pat);
    let mut save = [0u32; 1];
    let mut found = Vec::new();
    while matches.next(&mut save) {
        found.push(save[0]);
    }
    // More than one hit means the pattern no longer pins the function down.
    let [rva] = found[..] else {
        return Err(format!("warp pattern matched {} times", found.len()));
    };
    let va = program.rva_to_va(rva + 2).map_err(|e| format!("{e}"))?;
    Ok(unsafe { std::mem::transmute::<u64, WarpFn>(va) })
});

/// Warps the player to the grace with this BonfireWarpParam row id. Must run on
/// the main thread.
pub fn to_grace(row_id: u32) -> Result<u32, String> {
    let warp = WARP.as_ref().map_err(Clone::clone)?;
    let params = unsafe { SoloParamRepository::instance() }.map_err(|e| format!("{e:?}"))?;
    let row = params
        .get::<BonfireWarpParam>(row_id)
        .ok_or_else(|| format!("no BonfireWarpParam row {row_id}"))?;
    let entity = row.bonfire_entity_id();
    let lua = unsafe { CSLuaEventManImp::instance() }.map_err(|e| format!("{e:?}"))?;
    let imitation = lua
        .lua_event_script_imitation
        .as_ref()
        .ok_or("no lua event script imitation yet")?;
    warp(
        imitation.as_ptr() as usize,
        lua.lua_event_proxy.as_ptr() as usize,
        entity,
    );
    Ok(entity)
}
