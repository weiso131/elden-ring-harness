use fromsoftware_shared::program::Program;
use pelite::pattern;
use pelite::pe64::Pe;
use std::ffi::c_void;
use std::sync::LazyLock;

/// The game's own item grant, the one picking an item up goes through, and
/// the pointer to MapItemMan that it takes. Neither is bound by
/// fromsoftware-rs; the patterns and the arguments are the ones
/// eldenring-practice-tool (veeenu) uses for 2.7.2.
const SPAWN_PATTERN: &str = "48 8B C4 56 57 41 56 48 81 EC ? ? ? ? 48 C7 44 24 ? ? ? ? ? \
    48 89 58 ? 48 89 68 ? 48 8B 05 ? ? ? ? 48 33 C4 48 89 84 24 ? ? ? ? 41 0F B6 F9";
/// A `mov rcx, [rip+disp32]` loading MapItemMan.
const MAN_PATTERN: &str =
    "48 8B 0D ? ? ? ? C7 44 24 50 FF FF FF FF C7 45 A0 FF FF FF FF 48 85 C9 75 2E";

#[repr(C)]
struct Request {
    count: u32,
    id: u32,
    quantity: u32,
    durability: i32,
    gem: i32,
}

type SpawnFn = extern "C" fn(man: *const c_void, req: *mut Request, out: *mut u32, zero: u32);

struct Spawn {
    func: SpawnFn,
    man: *const *const c_void,
    rvas: (u32, u32),
}

unsafe impl Send for Spawn {}
unsafe impl Sync for Spawn {}

fn find(pat: &str) -> Result<u32, String> {
    let program = Program::current();
    let pat = pattern::parse(pat).map_err(|e| format!("bad pattern: {e}"))?;
    let mut matches = program.scanner().matches_code(&pat);
    let mut save = [0u32; 1];
    let mut found = Vec::new();
    while matches.next(&mut save) {
        found.push(save[0]);
    }
    match found[..] {
        [rva] => Ok(rva),
        _ => Err(format!("pattern matched {} times", found.len())),
    }
}

static SPAWN: LazyLock<Result<Spawn, String>> = LazyLock::new(|| {
    let program = Program::current();
    let va = |rva| program.rva_to_va(rva).map_err(|e| format!("{e}"));
    let func = find(SPAWN_PATTERN).map_err(|e| format!("item spawn: {e}"))?;
    let load = find(MAN_PATTERN).map_err(|e| format!("MapItemMan: {e}"))?;
    let disp = unsafe { ((va(load)? + 3) as *const i32).read_unaligned() };
    let man = (load as i64 + 7 + disp as i64) as u32;
    Ok(Spawn {
        func: unsafe { std::mem::transmute::<u64, SpawnFn>(va(func)?) },
        man: va(man)? as *const *const c_void,
        rvas: (func, man),
    })
});

/// Puts `quantity` of item `id` in the player's inventory. `id` carries the
/// category in its top bits (weapons 0, armor 0x10000000, talismans
/// 0x20000000, goods 0x40000000, ashes of war 0x80000000) and a weapon's
/// upgrade level in its last digits. Must run on the main thread.
pub fn give(id: u32, quantity: u32) -> Result<String, String> {
    let s = SPAWN.as_ref().map_err(Clone::clone)?;
    let man = unsafe { *s.man };
    if man.is_null() {
        return Err("MapItemMan not created yet".into());
    }
    let mut req = Request {
        count: 1,
        id,
        quantity,
        durability: -1,
        gem: -1,
    };
    let mut out = 0u32;
    (s.func)(man, &mut req, &mut out, 0);
    Ok(format!(
        "func_rva=0x{:x} map_item_man_rva=0x{:x} out={out}",
        s.rvas.0, s.rvas.1
    ))
}
