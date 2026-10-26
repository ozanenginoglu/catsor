// Embeds the app icon into catsor.exe so Explorer, Task Manager and shortcuts show the cat instead
// of the generic executable icon. `assets/catsor.ico` is generated from `render_tray_icon`:
// `cargo test regenerate_app_icons -- --ignored`.
//
// The compiled resource file (.res) is written directly instead of going through an .rc script.
// That needs no resource compiler on MSVC, and on GNU it skips windres' C preprocessor step,
// which breaks on paths that contain spaces.
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const ICON_PATH: &str = "assets/catsor.ico";
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const LANG_EN_US: u16 = 0x0409;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={ICON_PATH}");
    println!("cargo:rerun-if-env-changed=WINDRES");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let ico = fs::read(manifest_dir.join(ICON_PATH)).expect("failed to read assets/catsor.ico");
    let res_path = out_dir.join("catsor.res");
    fs::write(&res_path, icon_res(&ico)).expect("failed to write catsor.res");

    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        // link.exe and lld-link accept .res files as linker input
        println!("cargo:rustc-link-arg-bins={}", res_path.display());
        return;
    }

    // MinGW linkers want a COFF object. Converting a .res with windres doesn't run the preprocessor.
    let obj_path = out_dir.join("catsor_res.o");
    for windres in windres_candidates() {
        let converted = Command::new(&windres)
            .args(["--input-format=res", "--output-format=coff", "--input"])
            .arg(&res_path)
            .arg("--output")
            .arg(&obj_path)
            .status();
        if matches!(converted, Ok(status) if status.success()) {
            println!("cargo:rustc-link-arg-bins={}", obj_path.display());
            return;
        }
    }
    println!(
        "cargo:warning=windres not found; catsor.exe is built without its icon (set WINDRES to its path)"
    );
}

/// Explicit `WINDRES`, then the usual MinGW and LLVM names
fn windres_candidates() -> Vec<String> {
    if let Ok(windres) = env::var("WINDRES") {
        return vec![windres];
    }
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    vec![
        "windres".into(),
        format!("{arch}-w64-mingw32-windres"),
        "llvm-windres".into(),
    ]
}

/// Builds a .res holding each .ico image as an RT_ICON (ids 1..) plus RT_GROUP_ICON 1 indexing them
fn icon_res(ico: &[u8]) -> Vec<u8> {
    let u16_at = |at: usize| u16::from_le_bytes([ico[at], ico[at + 1]]);
    let u32_at = |at: usize| u32::from_le_bytes(ico[at..at + 4].try_into().unwrap());
    assert!(
        ico.len() >= 6 && u16_at(0) == 0 && u16_at(2) == 1,
        "{ICON_PATH} is not an .ico file"
    );
    let count = u16_at(4);

    let mut res = Vec::new();
    // A .res file starts with an empty entry
    push_resource(&mut res, 0, 0, 0, 0, &[]);

    let mut group = Vec::new();
    group.extend_from_slice(&0u16.to_le_bytes());
    group.extend_from_slice(&1u16.to_le_bytes());
    group.extend_from_slice(&count.to_le_bytes());
    for i in 0..count {
        let entry = 6 + 16 * i as usize;
        let size = u32_at(entry + 8) as usize;
        let offset = u32_at(entry + 12) as usize;
        let id = i + 1;
        push_resource(
            &mut res,
            RT_ICON,
            id,
            0x1010,
            LANG_EN_US,
            &ico[offset..offset + size],
        );
        // GRPICONDIRENTRY: the ICONDIRENTRY's first 12 bytes, then the RT_ICON id instead of the offset
        group.extend_from_slice(&ico[entry..entry + 12]);
        group.extend_from_slice(&id.to_le_bytes());
    }
    push_resource(&mut res, RT_GROUP_ICON, 1, 0x1030, LANG_EN_US, &group);
    res
}

/// Appends one RESOURCEHEADER (numeric type and name) and its DWORD-padded data
fn push_resource(res: &mut Vec<u8>, kind: u16, id: u16, flags: u16, lang: u16, data: &[u8]) {
    const HEADER_SIZE: u32 = 32;
    res.extend_from_slice(&(data.len() as u32).to_le_bytes());
    res.extend_from_slice(&HEADER_SIZE.to_le_bytes());
    for ordinal in [kind, id] {
        res.extend_from_slice(&0xFFFFu16.to_le_bytes());
        res.extend_from_slice(&ordinal.to_le_bytes());
    }
    res.extend_from_slice(&0u32.to_le_bytes()); // DataVersion
    res.extend_from_slice(&flags.to_le_bytes());
    res.extend_from_slice(&lang.to_le_bytes());
    res.extend_from_slice(&0u32.to_le_bytes()); // Version
    res.extend_from_slice(&0u32.to_le_bytes()); // Characteristics
    res.extend_from_slice(data);
    res.resize(res.len().next_multiple_of(4), 0);
}
