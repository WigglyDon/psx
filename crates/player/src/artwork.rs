//! Small, local library artwork. Never downloads a game's images or publishes
//! card contents. Format reference: psx-spx memory-card data format.
use crate::saves::CARD_SIZE;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

#[derive(Clone)]
pub struct SaveIcon {
    pub frames: Vec<[[u8; 4]; 256]>,
}
#[derive(Clone)]
pub struct Artwork {
    pub color: [f32; 3],
    pub initial: char,
    pub icon: Option<SaveIcon>,
}
impl Artwork {
    pub fn local(id: &str, title: &str, card: &Path) -> Self {
        let seed = id
            .bytes()
            .fold(0u32, |n, b| n.wrapping_mul(31).wrapping_add(b as u32));
        let colors = [
            [0.17, 0.46, 0.47],
            [0.37, 0.31, 0.55],
            [0.57, 0.32, 0.24],
            [0.24, 0.40, 0.61],
            [0.41, 0.45, 0.22],
            [0.52, 0.28, 0.43],
        ];
        Self {
            color: colors[seed as usize % colors.len()],
            initial: title.chars().find(|c| c.is_alphanumeric()).unwrap_or('?'),
            icon: card_icon(card),
        }
    }
}

/// Read only allocated first blocks. No card writes, file-title guesses or
/// traversal of a possibly damaged chain. Palette zero is transparent; bit15
/// does not make a black pixel transparent. At most three 16x16 frames.
/// https://psx-spx.consoledev.net/dev/ps1/sio/controllersandmemorycards/memory-card-data-format/
pub fn card_icon(path: &Path) -> Option<SaveIcon> {
    let mut file = File::open(path).ok()?;
    if file.metadata().ok()?.len() != CARD_SIZE as u64 {
        return None;
    }
    let mut directory = [0; 2048];
    file.read_exact(&mut directory).ok()?;
    if &directory[..2] != b"MC" {
        return None;
    }
    for block in 1..16 {
        let entry = &directory[block * 128..(block + 1) * 128];
        if entry[0] != 0x51 || entry[..127].iter().fold(0u8, |a, b| a ^ b) != entry[127] {
            continue;
        }
        file.seek(SeekFrom::Start(block as u64 * 8192)).ok()?;
        let mut header = [0; 128];
        file.read_exact(&mut header).ok()?;
        if &header[..2] != b"SC" || !(0x11..=0x13).contains(&header[2]) {
            continue;
        }
        let count = header[2] as usize - 0x10;
        let palette: [[u8; 4]; 16] = std::array::from_fn(|i| {
            let value = u16::from_le_bytes([header[96 + i * 2], header[97 + i * 2]]);
            let channel = |shift: u32| (((value >> shift) & 31u16) * 255 / 31) as u8;
            [
                channel(0),
                channel(5),
                channel(10),
                if value == 0 { 0 } else { 255 },
            ]
        });
        let mut frames = Vec::with_capacity(count);
        for _ in 0..count {
            let mut pixels = [0u8; 128];
            file.read_exact(&mut pixels).ok()?;
            frames.push(std::array::from_fn(|i| {
                palette[((pixels[i / 2] >> (i % 2 * 4)) & 15) as usize]
            }));
        }
        return Some(SaveIcon { frames });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_allocated_icons_decode_with_correct_color_and_transparency() {
        let path = std::env::temp_dir().join(format!("psx-icon-{}", std::process::id()));
        let mut bytes = vec![0; CARD_SIZE];
        bytes[..2].copy_from_slice(b"MC");
        bytes[128] = 0x51;
        bytes[255] = 0x51;
        bytes[8192..8194].copy_from_slice(b"SC");
        bytes[8194] = 0x12;
        bytes[8192 + 98] = 31;
        bytes[8192 + 128] = 0x01;
        std::fs::write(&path, &bytes).unwrap();
        let icon = card_icon(&path).unwrap();
        assert_eq!(icon.frames.len(), 2);
        assert_eq!(icon.frames[0][0], [255, 0, 0, 255]);
        assert_eq!(icon.frames[0][1], [0, 0, 0, 0]);
        bytes[128] = 0xa1;
        bytes[255] = 0xa1;
        std::fs::write(&path, &bytes).unwrap();
        assert!(card_icon(&path).is_none());
        std::fs::remove_file(path).unwrap();
    }
}
