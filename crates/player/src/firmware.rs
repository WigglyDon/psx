//! The free fallback boots directly from CD through OpenBIOS's own build mode.
use crate::saves::atomic_write;
use std::{fs, io, path::Path};

const IMAGE: &[u8] = include_bytes!("../../../vendor/openbios/openbios-fast.bin");

pub fn prepare(system: &Path) -> io::Result<()> {
    fs::create_dir_all(system)?;
    let destination = system.join("openbios.bin");
    // Supplied firmware remains authoritative. The donor still prefers its
    // configured regional firmware over directory discovery of this fallback.
    if !destination.exists() {
        atomic_write(&destination, IMAGE)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn free_fallback_is_pinned_and_never_replaces_supplied_firmware() {
        assert_eq!(IMAGE.len(), 512 * 1024);
        assert_eq!(&IMAGE[0x78..0x80], b"OpenBIOS");
        assert_eq!(
            format!("{:x}", Sha256::digest(IMAGE)),
            "c13794bc14de130c363786f45710d6e472c092070c0524ddede50f41f948bc45"
        );
        let root = std::env::temp_dir().join(format!("psx-free-bios-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        prepare(&root).unwrap();
        assert_eq!(fs::read(root.join("openbios.bin")).unwrap(), IMAGE);
        fs::write(root.join("openbios.bin"), b"owner firmware").unwrap();
        fs::write(root.join("scph5501.bin"), b"owner regional firmware").unwrap();
        prepare(&root).unwrap();
        assert_eq!(
            fs::read(root.join("openbios.bin")).unwrap(),
            b"owner firmware"
        );
        assert_eq!(
            fs::read(root.join("scph5501.bin")).unwrap(),
            b"owner regional firmware"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
