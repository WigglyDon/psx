//! Bounded local content inspection. A shared disc container is not a console.
use crate::files::{self, System};
use std::{
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};

fn at(file: &mut File, offset: u64, size: usize) -> io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![0; size];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}
fn sectors(
    file: &mut File,
    extent: u32,
    length: usize,
    stride: u64,
    header: u64,
) -> io::Result<Vec<u8>> {
    if length > 512 * 1024 {
        return Err(io::Error::other(
            "Disc metadata exceeds the supported inspection bound.",
        ));
    }
    let mut bytes = Vec::with_capacity(length);
    for sector in 0..length.div_ceil(2048) {
        let offset = (extent as u64 + sector as u64)
            .checked_mul(stride)
            .and_then(|n| n.checked_add(header))
            .ok_or_else(|| io::Error::other("Invalid disc extent."))?;
        bytes.extend(at(file, offset, (length - bytes.len()).min(2048))?);
    }
    Ok(bytes)
}
fn config_system(bytes: &[u8]) -> System {
    let text = String::from_utf8_lossy(bytes);
    let mut found = System::Unknown;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if value.trim().is_empty() {
            continue;
        }
        let current = if key.trim().eq_ignore_ascii_case("BOOT2") {
            System::Ps2
        } else if key.trim().eq_ignore_ascii_case("BOOT") {
            System::Ps1
        } else {
            continue;
        };
        if found != System::Unknown && found != current {
            return System::Unknown;
        }
        found = current;
    }
    found
}

/// An ISO directory entry, after validating its bounded record and name.
fn directory_entry(bytes: &[u8], wanted: &[u8]) -> Option<(u32, usize, bool)> {
    let mut offset = 0;
    while offset < bytes.len() {
        let size = bytes[offset] as usize;
        if size == 0 {
            offset = (offset / 2048 + 1) * 2048;
            continue;
        }
        let record = bytes.get(offset..offset.checked_add(size)?)?;
        if size < 34 || 33 + record[32] as usize > size {
            return None;
        }
        let name = &record[33..33 + record[32] as usize];
        let name = name.strip_suffix(b";1").unwrap_or(name);
        if name.eq_ignore_ascii_case(wanted) {
            return Some((
                u32::from_le_bytes(record[2..6].try_into().ok()?),
                u32::from_le_bytes(record[10..14].try_into().ok()?) as usize,
                record[25] & 2 != 0,
            ));
        }
        offset += size;
    }
    None
}

fn psp_elf(bytes: &[u8]) -> bool {
    bytes.starts_with(b"~PSP")
        || (bytes.len() >= 20
            && bytes.starts_with(b"\x7fELF\x01\x01")
            && u16::from_le_bytes([bytes[18], bytes[19]]) == 8)
}

fn optical(path: &Path) -> io::Result<System> {
    let mut file = File::open(path)?;
    for (stride, header) in [(2048, 0), (2352, 24), (2352, 16), (2336, 8)] {
        let Ok(pvd) = at(&mut file, 16 * stride + header, 2048) else {
            continue;
        };
        if pvd[0] != 1 || &pvd[1..6] != b"CD001" || pvd[6] != 1 {
            continue;
        }
        let root = &pvd[156..];
        if root[0] < 34 {
            return Ok(System::Unknown);
        }
        let extent = u32::from_le_bytes(root[2..6].try_into().unwrap());
        let length = u32::from_le_bytes(root[10..14].try_into().unwrap()) as usize;
        let directory = sectors(&mut file, extent, length, stride, header)?;
        let mut system = System::Unknown;
        if let Some((extent, length, false)) = directory_entry(&directory, b"SYSTEM.CNF") {
            if length > 16 * 1024 {
                return Ok(System::Unknown);
            }
            system = config_system(&sectors(&mut file, extent, length, stride, header)?);
        }
        if let Some((extent, length, true)) = directory_entry(&directory, b"PSP_GAME") {
            let psp = sectors(&mut file, extent, length, stride, header)?;
            if let Some((extent, length, false)) = directory_entry(&psp, b"PARAM.SFO") {
                if (20..=64 * 1024).contains(&length)
                    && sectors(&mut file, extent, 20, stride, header)?.starts_with(b"\0PSF")
                    && directory_entry(&directory, b"UMD_DATA.BIN").is_some_and(|e| !e.2)
                {
                    return Ok(if system == System::Unknown {
                        System::Psp
                    } else {
                        // Conflicting boot metadata must never select a core.
                        System::Unknown
                    });
                }
            }
        }
        return Ok(system);
    }
    Err(io::Error::other(
        "This is not a recognized optical-disc image.",
    ))
}

/// Structural recognition and boot metadata, not an emulation-compatibility claim.
/// CHD's header identifies its container only; unresolved games require a choice.
pub fn inspect(path: &Path) -> io::Result<(System, String)> {
    match files::extension(&path.to_string_lossy()).as_str() {
        "cue" => {
            if fs::metadata(path)?.len() > 1024 * 1024 {
                return Err(io::Error::other("The CUE is too large."));
            }
            let text = fs::read_to_string(path)?;
            let mut found = None;
            let mut volume = false;
            for track in files::cue_tracks(&text)? {
                let track = path
                    .parent()
                    .ok_or_else(|| io::Error::other("The CUE has no directory."))?
                    .join(track);
                if !track.is_file() {
                    return Err(io::Error::other("A CUE track is missing."));
                }
                if let Ok(system) = optical(&track) {
                    volume = true;
                    if system != System::Unknown {
                        if found.is_some_and(|s| s != system) {
                            return Err(io::Error::other(
                                "The CUE contains conflicting console metadata.",
                            ));
                        }
                        found = Some(system);
                    }
                }
            }
            if !volume {
                return Err(io::Error::other("No recognized data track was found."));
            }
            let system = found.unwrap_or(System::Unknown);
            Ok((system, format!("{} · CUE", system.label())))
        }
        "iso" | "bin" => {
            let system = optical(path)?;
            Ok((system, format!("{} · disc image", system.label())))
        }
        "chd" => {
            let mut file = File::open(path)?;
            let header = at(&mut file, 0, 16)?;
            let version = u32::from_be_bytes(header[12..16].try_into().unwrap());
            let size = match version {
                3 => 120,
                4 => 108,
                5 => 124,
                _ => 0,
            };
            if &header[..8] != b"MComprHD"
                || size == 0
                || u32::from_be_bytes(header[8..12].try_into().unwrap()) != size
                || file.metadata()?.len() < size as u64
            {
                return Err(io::Error::other(
                    "The CHD header is unsupported or incomplete.",
                ));
            }
            Ok((System::Unknown, "CHD · choose console".into()))
        }
        "exe" => {
            let mut file = File::open(path)?;
            if at(&mut file, 0, 8)? == b"PS-X EXE" && file.metadata()?.len() >= 2048 {
                Ok((System::Ps1, "PS-X EXE".into()))
            } else {
                Err(io::Error::other("This is not a PS-X executable."))
            }
        }
        "pbp" => {
            let mut file = File::open(path)?;
            let header = at(&mut file, 0, 40)?;
            let length = file.metadata()?.len();
            let offsets: Vec<_> = header[8..]
                .chunks_exact(4)
                .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as u64)
                .collect();
            if &header[..4] != b"\0PBP"
                || offsets[0] < 40
                || offsets.iter().any(|&o| o > length)
                || offsets.windows(2).any(|w| w[0] > w[1])
            {
                return Err(io::Error::other("The PBP section table is invalid."));
            }
            let psar = at(
                &mut file,
                offsets[7],
                (length - offsets[7]).min(16) as usize,
            )?;
            if psar.starts_with(b"PSISOIMG0000") || psar.starts_with(b"PSTITLEIMG000000") {
                Ok((System::Ps1, "PS1 PBP".into()))
            } else if offsets[7] - offsets[6] >= 20 && psp_elf(&at(&mut file, offsets[6], 20)?) {
                Ok((System::Psp, "PSP PBP".into()))
            } else {
                Err(io::Error::other(
                    "This PBP contains no recognized PSP executable or PS1 image.",
                ))
            }
        }
        "cso" => {
            let mut file = File::open(path)?;
            let b = at(&mut file, 0, 24)?;
            let size = u64::from_le_bytes(b[8..16].try_into().unwrap());
            let block = u32::from_le_bytes(b[16..20].try_into().unwrap());
            let header = u32::from_le_bytes(b[4..8].try_into().unwrap()) as u64;
            if &b[..4] != b"CISO"
                || !(24..=4096).contains(&header)
                || size == 0
                || block != 2048
                || b[20] > 2
                || b[21] > 31
                || header
                    .checked_add((size.div_ceil(2048) + 1).saturating_mul(4))
                    .is_none_or(|n| n > file.metadata().map(|m| m.len()).unwrap_or(0))
            {
                return Err(io::Error::other(
                    "The CSO header or block index is invalid.",
                ));
            }
            // CSO is also used for other systems. Compressed boot metadata has
            // not been inspected; keep the same honest choice as unresolved CHD.
            Ok((System::Unknown, "CSO · choose console".into()))
        }
        _ => Err(io::Error::other(
            "Choose a supported disc image. Extract archives first.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn psp_and_ps1_pbp_share_a_container_but_not_a_boot_payload() {
        let root = std::env::temp_dir().join(format!("psx-pbp-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("game.pbp");
        let mut bytes = vec![0u8; 100];
        bytes[..4].copy_from_slice(b"\0PBP");
        for chunk in bytes[8..40].chunks_exact_mut(4) {
            chunk.copy_from_slice(&40u32.to_le_bytes());
        }
        bytes[36..40].copy_from_slice(&64u32.to_le_bytes());
        bytes[40..44].copy_from_slice(b"~PSP");
        fs::write(&path, &bytes).unwrap();
        assert_eq!(inspect(&path).unwrap().0, System::Psp);
        // A PS1 EBOOT also contains a PSP bootstrap. PSAR must take priority.
        bytes[64..76].copy_from_slice(b"PSISOIMG0000");
        fs::write(&path, &bytes).unwrap();
        assert_eq!(inspect(&path).unwrap().0, System::Ps1);
        bytes[36..40].copy_from_slice(&400u32.to_le_bytes());
        fs::write(&path, &bytes).unwrap();
        assert!(inspect(&path).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn psp_iso_needs_umd_and_game_metadata_and_refuses_conflicting_boot() {
        fn entry(name: &[u8], extent: u32, len: u32, directory: bool) -> Vec<u8> {
            let mut r = vec![0; (33 + name.len() + 1) & !1];
            r[0] = r.len() as u8;
            r[2..6].copy_from_slice(&extent.to_le_bytes());
            r[10..14].copy_from_slice(&len.to_le_bytes());
            r[25] = if directory { 2 } else { 0 };
            r[32] = name.len() as u8;
            r[33..33 + name.len()].copy_from_slice(name);
            r
        }
        let root = std::env::temp_dir().join(format!("psx-umd-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("game.iso");
        let mut bytes = vec![0u8; 25 * 2048];
        bytes[16 * 2048..16 * 2048 + 7].copy_from_slice(b"\x01CD001\x01");
        let e = entry(&[0], 20, 2048, true);
        bytes[16 * 2048 + 156..16 * 2048 + 156 + e.len()].copy_from_slice(&e);
        let e = entry(b"PSP_GAME", 21, 2048, true);
        bytes[20 * 2048..20 * 2048 + e.len()].copy_from_slice(&e);
        let cursor = 20 * 2048 + e.len();
        let e = entry(b"PARAM.SFO;1", 22, 20, false);
        bytes[21 * 2048..21 * 2048 + e.len()].copy_from_slice(&e);
        bytes[22 * 2048..22 * 2048 + 4].copy_from_slice(b"\0PSF");
        fs::write(&path, &bytes).unwrap();
        assert_eq!(inspect(&path).unwrap().0, System::Unknown);
        let e = entry(b"UMD_DATA.BIN;1", 23, 32, false);
        bytes[cursor..cursor + e.len()].copy_from_slice(&e);
        let cursor = cursor + e.len();
        fs::write(&path, &bytes).unwrap();
        assert_eq!(inspect(&path).unwrap().0, System::Psp);
        let config = b"BOOT2=cdrom0:\\SLUS_000.00;1";
        let e = entry(b"SYSTEM.CNF;1", 24, config.len() as u32, false);
        bytes[cursor..cursor + e.len()].copy_from_slice(&e);
        bytes[24 * 2048..24 * 2048 + config.len()].copy_from_slice(config);
        fs::write(&path, &bytes).unwrap();
        assert_eq!(inspect(&path).unwrap().0, System::Unknown);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn same_iso_container_and_volume_name_need_actual_boot_metadata() {
        let root = std::env::temp_dir().join(format!("psx-detection-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        for (name, config, expected) in [
            ("one.iso", "BOOT = cdrom:\\SLUS_000.00;1\r\n", System::Ps1),
            ("two.iso", "BOOT2 = cdrom0:\\SLUS_200.35;1\r\n", System::Ps2),
            ("unknown.iso", "VER = 1.00\n", System::Unknown),
        ] {
            let mut bytes = vec![0u8; 24 * 2048];
            let pvd = &mut bytes[16 * 2048..17 * 2048];
            pvd[..7].copy_from_slice(b"\x01CD001\x01");
            pvd[8..19].copy_from_slice(b"PLAYSTATION");
            pvd[156] = 34;
            pvd[158..162].copy_from_slice(&20u32.to_le_bytes());
            pvd[166..170].copy_from_slice(&2048u32.to_le_bytes());
            let record = &mut bytes[20 * 2048..21 * 2048];
            record[0] = 46;
            record[2..6].copy_from_slice(&21u32.to_le_bytes());
            record[10..14].copy_from_slice(&(config.len() as u32).to_le_bytes());
            record[32] = 12;
            record[33..45].copy_from_slice(b"SYSTEM.CNF;1");
            bytes[21 * 2048..21 * 2048 + config.len()].copy_from_slice(config.as_bytes());
            let path = root.join(name);
            fs::write(&path, bytes).unwrap();
            assert_eq!(inspect(&path).unwrap().0, expected);
        }
        assert_eq!(config_system(b"BOOT=a\nBOOT2=b"), System::Unknown);
        fs::remove_dir_all(root).unwrap();
    }
}
