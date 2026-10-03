use std::{env, fs, path::PathBuf};

fn resource(output: &mut Vec<u8>, kind: u16, name: u16, data: &[u8]) {
    // Ordinal Windows .res header, followed by DWORD-aligned resource data.
    output.extend_from_slice(&(data.len() as u32).to_le_bytes());
    output.extend_from_slice(&32u32.to_le_bytes());
    for value in [0xffffu16, kind, 0xffff, name] {
        output.extend_from_slice(&value.to_le_bytes());
    }
    output.extend_from_slice(&0u32.to_le_bytes());
    let (flags, language) = if kind == 0 {
        (0u16, 0u16)
    } else {
        (0x30, 0x409)
    };
    output.extend_from_slice(&flags.to_le_bytes());
    output.extend_from_slice(&language.to_le_bytes());
    output.extend_from_slice(&0u32.to_le_bytes());
    output.extend_from_slice(&0u32.to_le_bytes());
    output.extend_from_slice(data);
    while !output.len().is_multiple_of(4) {
        output.push(0);
    }
}

fn main() {
    println!("cargo:rerun-if-changed=../../assets/branding/AssetForge.ico");
    println!("cargo:rerun-if-env-changed=ASSET_FORGE_WINDOWS_MANIFEST");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let icon = fs::read(root.join("../../assets/branding/AssetForge.ico"))
        .expect("Bundled Windows icon must exist");
    assert_eq!(&icon[..4], &[0, 0, 1, 0], "Expected an ICO container");
    let count = u16::from_le_bytes(icon[4..6].try_into().unwrap());
    assert!((1..=32).contains(&count), "Expected multiple icon sizes");
    let mut output = vec![];
    resource(&mut output, 0, 0, &[]);
    let mut group = icon[..6].to_vec();
    for index in 0..usize::from(count) {
        let entry = &icon[6 + index * 16..22 + index * 16];
        let length = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize;
        let offset = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as usize;
        let id = index as u16 + 1;
        resource(&mut output, 3, id, &icon[offset..offset + length]);
        group.extend_from_slice(&entry[..12]);
        group.extend_from_slice(&id.to_le_bytes());
    }
    // GPUI loads group icon ID 1 for the native window and taskbar.
    resource(&mut output, 14, 1, &group);
    if let Some(manifest) = env::var_os("ASSET_FORGE_WINDOWS_MANIFEST") {
        println!(
            "cargo:rerun-if-changed={}",
            PathBuf::from(&manifest).display()
        );
        resource(
            &mut output,
            24,
            1,
            &fs::read(manifest).expect("Verified GPUI Windows manifest must exist"),
        );
        println!("cargo:rustc-link-arg-bin=asset-forge-studio=/manifest:no");
    }
    let path = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("asset-forge.res");
    fs::write(&path, output).expect("Windows icon resources must be writable");
    println!(
        "cargo:rustc-link-arg-bin=asset-forge-studio={}",
        path.display()
    );
}
