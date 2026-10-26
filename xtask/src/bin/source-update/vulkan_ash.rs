//! One pinned registry addition, streamed directly from its authenticated crate.
//! No copied/expanded dependency tree and no registry/network refresh.
use super::*;
pub const SELF_SOURCE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/bin/source-update/vulkan_ash.rs"
));
pub const VERSION: &str = "0.37.3+1.3.251";
pub const SHA: &str = "39e9c3835d686b0a6084ab4234fcd1b07dbf6e4767dce60874b12356a25ecd4a";
const RECORD: &str = "[[package]]\nname = \"ash\"\nversion = \"0.37.3+1.3.251\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"39e9c3835d686b0a6084ab4234fcd1b07dbf6e4767dce60874b12356a25ecd4a\"\n\n";
pub fn lock_change(before: &[u8], after: &[u8]) -> Result<()> {
    let after = std::str::from_utf8(after)?;
    if after.matches(RECORD).count() != 1 || after.matches(" \"ash\",\n").count() != 1 {
        return Err("Expected only the one pinned ash registry addition".into());
    }
    let removed = after.replacen(RECORD, "", 1).replacen(" \"ash\",\n", "", 1);
    if removed.as_bytes() != before {
        return Err("Cargo graph changed beyond pinned ash".into());
    }
    Ok(())
}
pub fn additions(crate_file: &Path) -> Result<BTreeMap<String, (u32, Vec<u8>)>> {
    if hash(crate_file)? != SHA {
        return Err("Pinned ash crate checksum differs".into());
    }
    let mut reader = Reader::open(crate_file)?;
    let mut additions = BTreeMap::new();
    let mut checksums = serde_json::Map::new();
    let mut seen = BTreeSet::new();
    let mut bytes = 0usize;
    let prefix = format!("ash-{VERSION}/");
    while let Some(entry) = reader.next()? {
        let original = ctext(unsafe { archive_entry_pathname(entry.0) })?;
        let relative = safe(
            original
                .strip_prefix(&prefix)
                .ok_or("Unexpected crate root")?,
        )?;
        let mode = unsafe { archive_entry_mode(entry.0) };
        if !seen.insert(relative.clone())
            || seen.len() > 512
            || ![0o100644, 0o100755].contains(&mode)
            || optional(unsafe { archive_entry_symlink(entry.0) })?.is_some()
            || optional(unsafe { archive_entry_hardlink(entry.0) })?.is_some()
            || relative == ".cargo-checksum.json"
        {
            return Err("Unsafe/duplicate/unsupported ash crate member".into());
        }
        let size = usize::try_from(unsafe { archive_entry_size(entry.0) })?;
        bytes = bytes.checked_add(size).ok_or("Crate size overflow")?;
        if bytes > 16 * 1024 * 1024 || size > 8 * 1024 * 1024 {
            return Err("Pinned crate source bound exceeded".into());
        }
        let mut body = vec![0; size];
        let mut used = 0;
        while used < size {
            let n = reader.read(&mut body[used..])?;
            if n == 0 {
                return Err("Truncated ash source".into());
            }
            used += n;
        }
        if reader.read(&mut [0; 1])? != 0 {
            return Err("Oversized ash source member".into());
        }
        checksums.insert(relative.clone(), json!(digest(&body)));
        let full = format!("vendor/rust/ash/{relative}");
        let mut parent = Path::new(&full).parent();
        while let Some(path) = parent {
            let path = path.to_str().ok_or("Crate directory encoding")?;
            if path == "vendor/rust" {
                break;
            }
            additions
                .entry(path.into())
                .or_insert((0o040755, Vec::new()));
            parent = Path::new(path).parent();
        }
        if additions.insert(full, (mode, body)).is_some() {
            return Err("Crate directory/file collision".into());
        }
    }
    reader.finish()?;
    for required in ["Cargo.toml", "src/lib.rs", "LICENSE-MIT", "LICENSE-APACHE"] {
        if !checksums.contains_key(required) {
            return Err(format!("Missing ash source/license: {required}").into());
        }
    }
    additions.insert(
        "vendor/rust/ash/.cargo-checksum.json".into(),
        (
            0o100644,
            serde_json::to_vec(&json!({"files":checksums,"package":SHA}))?,
        ),
    );
    Ok(additions)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_pinned_lock_change_is_accepted() {
        let before =
            b"version = 4\n\n[[package]]\nname = \"psx-player\"\ndependencies = [\n \"glow\",\n]\n";
        let after = format!(
            "{RECORD}{}",
            std::str::from_utf8(before)
                .unwrap()
                .replace(" \"glow\",", " \"ash\",\n \"glow\",")
        );
        lock_change(before, after.as_bytes()).unwrap();
        assert!(lock_change(before, after.replace(VERSION, "0.38.0").as_bytes()).is_err());
        assert!(lock_change(before, after.replace("glow", "different").as_bytes()).is_err());
        assert!(lock_change(before, before).is_err());
    }
    #[test]
    #[ignore = "requires the prepared exact cached crate input"]
    fn pinned_crate_stream_supplies_complete_sources_and_both_licenses() {
        let input = std::env::var_os("PSX_SOURCE_ASH_CRATE").expect("Prepared pinned crate");
        let entries = additions(Path::new(&input)).unwrap();
        assert!(entries.len() > 20);
        let checksum: Value =
            serde_json::from_slice(&entries["vendor/rust/ash/.cargo-checksum.json"].1).unwrap();
        assert_eq!(checksum["package"], SHA);
        for (name, (mode, body)) in &entries {
            if *mode & 0o170000 == 0o100000 && !name.ends_with("/.cargo-checksum.json") {
                assert_eq!(
                    checksum["files"][name.strip_prefix("vendor/rust/ash/").unwrap()],
                    digest(body)
                );
            }
        }
    }
}
