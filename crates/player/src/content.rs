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
        let mut offset = 0;
        while offset < directory.len() {
            let size = directory[offset] as usize;
            if size == 0 {
                offset = (offset / 2048 + 1) * 2048;
                continue;
            }
            let Some(record) = directory.get(offset..offset + size) else {
                break;
            };
            if size < 34 || 33 + record[32] as usize > size {
                break;
            }
            let name = &record[33..33 + record[32] as usize];
            if name.eq_ignore_ascii_case(b"SYSTEM.CNF;1")
                || name.eq_ignore_ascii_case(b"SYSTEM.CNF")
            {
                let extent = u32::from_le_bytes(record[2..6].try_into().unwrap());
                let length = u32::from_le_bytes(record[10..14].try_into().unwrap()) as usize;
                if length > 16 * 1024 {
                    return Ok(System::Unknown);
                }
                return sectors(&mut file, extent, length, stride, header)
                    .map(|b| config_system(&b));
            }
            offset += size;
        }
        return Ok(System::Unknown);
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
            let offset = u32::from_le_bytes(header[36..40].try_into().unwrap());
            let psar = at(&mut file, offset as u64, 16)?;
            if &header[..4] == b"\0PBP"
                && (psar.starts_with(b"PSISOIMG0000") || psar.starts_with(b"PSTITLEIMG000000"))
            {
                Ok((System::Ps1, "PS1 PBP".into()))
            } else {
                Err(io::Error::other(
                    "This PBP does not contain a recognized PS1 image.",
                ))
            }
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
