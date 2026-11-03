//! Relocate the pinned SwanStation state envelope after durable identity checks.
//! Layout: vendor/swanstation/src/core/save_state_version.h and the optional
//! DiskStateHeader in src/libretro/libretro_host_interface.cpp. Guest machine
//! bytes and the saved file remain untouched; only frontend media names change.
use crate::files::Game;
use std::{borrow::Cow, io, path::Path};

const HEADER: usize = 216;
const TRAILER: &[u8; 8] = b"SWANDISC";
fn invalid() -> io::Error {
    io::Error::other("The saved disc location could not be updated safely. Your saved session and card are kept.")
}
fn word(bytes: &[u8], at: usize) -> io::Result<usize> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or_else(invalid)?
            .try_into()
            .unwrap(),
    ) as usize)
}
fn put(bytes: &mut [u8], at: usize, value: usize) -> io::Result<()> {
    let value = u32::try_from(value).map_err(|_| invalid())?;
    bytes
        .get_mut(at..at + 4)
        .ok_or_else(invalid)?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}
fn mapped<'a>(path: &[u8], old: &Game, current: &'a Game) -> io::Result<Option<&'a Path>> {
    let path = std::str::from_utf8(path).map_err(|_| invalid())?;
    if path.contains('\0') {
        return Err(invalid());
    }
    let mut found = None;
    for (index, (before, after)) in old.disc_list().iter().zip(current.disc_list()).enumerate() {
        if before.path == Path::new(path) && before.path != after.path {
            // Borrow from the real current entry, not the temporary disc list.
            let next = if current.discs.is_empty() {
                current.path.as_path()
            } else {
                current.discs[index].path.as_path()
            };
            if found.is_some_and(|prior| prior != next) {
                return Err(invalid());
            }
            found = Some(next);
        }
    }
    Ok(found)
}
pub(crate) fn relocate<'a>(
    bytes: &'a [u8],
    old: &Game,
    current: &Game,
) -> io::Result<Cow<'a, [u8]>> {
    let before = old.disc_list();
    let after = current.disc_list();
    if old.system != crate::files::System::Ps1
        || current.system != old.system
        || old.id != current.id
        || old.last_disc != current.last_disc
        || before.len() != after.len()
        || before.iter().zip(&after).any(|(a, b)| a.id != b.id)
    {
        return Err(invalid());
    }
    if before.iter().zip(&after).all(|(a, b)| a.path == b.path) {
        return Ok(Cow::Borrowed(bytes));
    }
    if bytes.len() < HEADER
        || &bytes[..4] != b"DUCC"
        || word(bytes, 4)? != 57
        || word(bytes, 200)? != 0
    {
        return Err(invalid());
    }
    let data = word(bytes, 212)?;
    let end = data.checked_add(word(bytes, 208)?).ok_or_else(invalid)?;
    if data < HEADER || end > bytes.len() {
        return Err(invalid());
    }
    let trailer = bytes.get(end..end.saturating_add(8)) == Some(TRAILER.as_slice());
    let (at, length, length_field) = if trailer {
        let flags = word(bytes, end + 12)?;
        let length = word(bytes, end + 20)?;
        if word(bytes, end + 8)? != 1
            || flags & !7 != 0
            || (flags & 2 != 0) != (length == 0)
            || (word(bytes, 168)? != 0) != (flags & 3 == 0)
        {
            return Err(invalid());
        }
        (end.checked_add(24).ok_or_else(invalid)?, length, end + 20)
    } else {
        (word(bytes, 172)?, word(bytes, 168)?, 168)
    };
    if length == 0 {
        return Ok(Cow::Borrowed(bytes));
    }
    let limit = at.checked_add(length).ok_or_else(invalid)?;
    let path = bytes.get(at..limit).ok_or_else(invalid)?;
    if !trailer && (at < HEADER || limit > data) {
        return Err(invalid());
    }
    let Some(next) = mapped(path, old, current)? else {
        // Never guess a playlist/container member or reinterpret guest data.
        return Ok(Cow::Borrowed(bytes));
    };
    let next = next
        .to_str()
        .filter(|p| !p.contains('\0') && !p.is_empty())
        .ok_or_else(invalid)?
        .as_bytes();
    let size = if trailer {
        bytes
            .len()
            .checked_sub(length)
            .and_then(|n| n.checked_add(next.len()))
    } else {
        bytes.len().checked_add(next.len())
    }
    .ok_or_else(invalid)?;
    if size > crate::resume::MAX_STATE_SIZE {
        return Err(invalid());
    }
    let mut result = Vec::with_capacity(size);
    if trailer {
        result.extend_from_slice(&bytes[..at]);
        result.extend_from_slice(next);
        result.extend_from_slice(&bytes[limit..]);
    } else {
        // Append the new legacy media name without moving machine/screenshot
        // bodies or changing their offsets. Native loading seeks by the header.
        result.extend_from_slice(bytes);
        result.extend_from_slice(next);
        put(&mut result, 172, bytes.len())?;
    }
    put(&mut result, length_field, next.len())?;
    Ok(Cow::Owned(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn game(path: &str) -> Game {
        Game {
            system: crate::files::System::Ps1,
            id: "abc123".into(),
            title: "Fixture".into(),
            path: path.into(),
            discs: vec![],
            last_disc: 0,
        }
    }
    pub(super) fn state(path: &str, trailer: bool, flags: usize) -> Vec<u8> {
        let mut b = vec![0; HEADER];
        b[..4].copy_from_slice(b"DUCC");
        put(&mut b, 4, 57).unwrap();
        put(&mut b, 168, if flags & 3 == 0 { path.len() } else { 0 }).unwrap();
        put(&mut b, 172, HEADER).unwrap();
        b.extend_from_slice(path.as_bytes());
        let data = b.len();
        b.extend_from_slice(b"guest machine bytes: old.cue");
        put(&mut b, 208, b"guest machine bytes: old.cue".len()).unwrap();
        put(&mut b, 212, data).unwrap();
        if trailer {
            let end = b.len();
            b.extend_from_slice(TRAILER);
            b.resize(end + 24, 0);
            put(&mut b, end + 8, 1).unwrap();
            put(&mut b, end + 12, flags).unwrap();
            put(&mut b, end + 20, path.len()).unwrap();
            b.extend_from_slice(path.as_bytes());
        }
        b.extend_from_slice(&[0; 32]);
        b
    }
    #[test]
    fn rename_retargets_trailer_without_changing_guest_or_tray() {
        for flags in [0, 1] {
            let original = state("old.cue", true, flags);
            let updated = relocate(&original, &game("old.cue"), &game("new longer.cue")).unwrap();
            let end = word(&original, 212).unwrap() + word(&original, 208).unwrap();
            assert_eq!(&updated[..end + 20], &original[..end + 20]);
            assert_eq!(word(&updated, end + 20).unwrap(), 14);
            assert_eq!(&updated[end + 24..end + 38], b"new longer.cue");
            assert_eq!(&updated[end + 38..], &original[end + 31..]);
        }
    }
    #[test]
    fn legacy_rename_changes_only_location_fields_and_appends_name() {
        let original = state("old.cue", false, 0);
        let updated = relocate(&original, &game("old.cue"), &game("new.cue")).unwrap();
        assert_eq!(word(&updated, 172).unwrap(), original.len());
        assert_eq!(&updated[original.len()..], b"new.cue");
        assert_eq!(&updated[176..original.len()], &original[176..]);
    }
    #[test]
    fn unchanged_unknown_and_wrong_identity_do_not_redirect_content() {
        assert!(matches!(
            relocate(b"native opaque", &game("same"), &game("same")).unwrap(),
            Cow::Borrowed(_)
        ));
        let original = state("unrecognized.cue", true, 0);
        assert!(matches!(
            relocate(&original, &game("old.cue"), &game("new.cue")).unwrap(),
            Cow::Borrowed(_)
        ));
        let mut other = game("new.cue");
        other.id = "other".into();
        assert!(relocate(&original, &game("old.cue"), &other).is_err());
        let mut malformed = state("old.cue", true, 0);
        put(&mut malformed, 212, u32::MAX as usize).unwrap();
        assert!(relocate(&malformed, &game("old.cue"), &game("new.cue")).is_err());
    }
    #[test]
    fn disc_order_and_tray_flags_survive_relocation() {
        let mut old = game("first.cue");
        old.discs = vec![
            crate::files::Disc {
                id: "one".into(),
                label: "one".into(),
                number: 1,
                path: "first.cue".into(),
            },
            crate::files::Disc {
                id: "two".into(),
                label: "two".into(),
                number: 2,
                path: "second.cue".into(),
            },
        ];
        let mut current = old.clone();
        current.discs[1].path = "renamed second.cue".into();
        let mut original = state("second.cue", true, 1);
        let end = word(&original, 212).unwrap() + word(&original, 208).unwrap();
        put(&mut original, end + 16, 1).unwrap();
        let updated = relocate(&original, &old, &current).unwrap();
        assert_eq!(word(&updated, end + 12).unwrap(), 1);
        assert_eq!(word(&updated, end + 16).unwrap(), 1);
        assert_eq!(&updated[end + 24..end + 42], b"renamed second.cue");
        current.discs.swap(0, 1);
        assert!(relocate(&original, &old, &current).is_err());
        let no_disc = state("", true, 2);
        let other = game("renamed.cue");
        assert!(matches!(
            relocate(&no_disc, &game("old.cue"), &other).unwrap(),
            Cow::Borrowed(_)
        ));
    }
    #[test]
    fn malformed_or_other_native_format_never_gets_rewritten() {
        let original = state("old.cue", true, 0);
        let end = word(&original, 212).unwrap() + word(&original, 208).unwrap();
        for (at, value) in [
            (4, 56),
            (200, 1),
            (end + 8, 2),
            (end + 12, 8),
            (end + 20, u32::MAX as usize),
        ] {
            let mut bad = original.clone();
            put(&mut bad, at, value).unwrap();
            assert!(relocate(&bad, &game("old.cue"), &game("new.cue")).is_err());
        }
    }
    #[test]
    fn actual_resume_gate_checks_identity_and_newer_card_before_retargeting() {
        let root = std::env::temp_dir().join(format!("psx-ps1-relocation-{}", std::process::id()));
        let old = game("old.cue");
        let current = game("renamed.cue");
        let native = state("old.cue", true, 0);
        crate::resume::save(&root, &old, true, b"card", &native).unwrap();
        let file = root.join("resume/abc123.state");
        let original = std::fs::read(&file).unwrap();
        let checkpoint = crate::resume::load(&root, &current, b"card")
            .unwrap()
            .unwrap();
        let retargeted = checkpoint.native_bytes_for(&current).unwrap();
        assert_ne!(retargeted.as_ref(), native);
        assert!(checkpoint.metadata.paused);
        assert!(crate::resume::load(&root, &current, b"newer card").is_err());
        assert_eq!(std::fs::read(&file).unwrap(), original);
        std::fs::remove_dir_all(root).unwrap();
    }
}
