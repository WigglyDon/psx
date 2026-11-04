use super::*;
pub(super) const RUST_ENTRY: &str = "lib/arm64-v8a/libpsx.so";
pub(super) const CAPABILITY: &str = "assets/development-player-acceptance.json";
fn signature(name: &str) -> bool {
    [
        "META-INF/PSX-DEVE.SF",
        "META-INF/PSX-DEVE.RSA",
        "META-INF/MANIFEST.MF",
    ]
    .contains(&name)
}
pub(super) fn names(list: &str) -> Result<Vec<String>> {
    let mut set = std::collections::BTreeSet::new();
    for name in list.lines() {
        if name.is_empty()
            || name.len() > 1024
            || name.contains('\\')
            || name.bytes().any(|b| b.is_ascii_control())
            || !Path::new(name)
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
            || !set.insert(name.to_owned())
            || set.len() > 10000
        {
            return Err("Unsafe or duplicate ZIP entry".into());
        }
    }
    if set.is_empty() {
        return Err("Empty APK".into());
    }
    Ok(set.into_iter().collect())
}
pub(super) fn baseline(entries: &BTreeMap<String, String>) -> Result<()> {
    for (name, sha) in [
        (
            "lib/arm64-v8a/libswanstation.so",
            "648d52710f7da65c8ab6b63e5a3002de672c6278419fbd2c6296c3cfeea13caf",
        ),
        (
            "lib/arm64-v8a/libarmsx2.so",
            "5fc44f05731b85a958d78e1ac20acf60b8d5c3955ae046cbe6ae1057e8734778",
        ),
        (
            "lib/arm64-v8a/libppsspp.so",
            "e40e92176d5f5844036944c13b89cad370fcf7931e1723f829c6376dcac71969",
        ),
        (
            RUST_ENTRY,
            "2bc79b9774374c454029a3f380bbecce602de5664788ac4986ccfb29804c2bb2",
        ),
    ] {
        if entries.get(name).map(String::as_str) != Some(sha) {
            return Err(format!("Frozen native mismatch: {name}").into());
        }
    }
    for name in [
        CAPABILITY,
        "classes.dex",
        "AndroidManifest.xml",
        "resources.arsc",
        "res/drawable/ic_psx.xml",
        "res/mipmap-anydpi-v26/ic_launcher.xml",
        "assets/notices/ppsspp/LICENSE.TXT",
        "assets/psp/resources/ppge_atlas.zim",
        "assets/psp/resources/compat.ini",
        "assets/psp/resources.tsv",
        "assets/psp/native-binding.json",
        "assets/notices/armsx2/COPYING.GPLv3",
        "assets/notices/host/LICENSE",
        "assets/notices/swanstation/LICENSE",
        "assets/notices/swanstation/dep/openbios/LICENSE.txt",
        "assets/notices/openbios/LICENSE",
        "assets/notices/openbios/UPSTREAM.md",
        "assets/notices/fonts/Barlow-OFL.txt",
        "assets/ps2/resources/patches.zip",
    ] {
        if !entries.contains_key(name) {
            return Err(format!("Missing required frozen payload: {name}").into());
        }
    }
    if entries
        .keys()
        .any(|n| n.starts_with("assets/development-") && n != CAPABILITY)
    {
        return Err("Unknown development capability".into());
    }
    Ok(())
}
pub(super) fn permitted(
    base: &BTreeMap<String, String>,
    actual: &BTreeMap<String, String>,
    rust: &str,
) -> Result<()> {
    let mut expected = base.clone();
    expected.remove(CAPABILITY);
    expected.retain(|name, _| !signature(name));
    expected.insert(RUST_ENTRY.into(), rust.into());
    let payload: BTreeMap<_, _> = actual
        .iter()
        .filter(|(n, _)| !signature(n))
        .map(|(n, s)| (n.clone(), s.clone()))
        .collect();
    if expected != payload {
        return Err(
            "APK payload differs beyond Rust replacement and acceptance marker removal".into(),
        );
    }
    Ok(())
}
pub(super) fn signer(output: &str, expected: &str) -> Result<()> {
    let values: Vec<_> = output
        .lines()
        .filter_map(|l| l.strip_prefix("Signer #1 certificate SHA-256 digest: "))
        .collect();
    if values.len() != 1 || values[0] != expected || output.contains("Signer #2 certificate") {
        return Err("APK signing identity differs".into());
    }
    Ok(())
}
pub(super) fn coherence(changes: &[u8], old: &[u8], new: &[u8]) -> Result<()> {
    let list = std::str::from_utf8(changes)?;
    if list.lines().collect::<Vec<_>>() != ["crates/player/src/resume.rs", "docs/architecture.md"] {
        return Err(
            "Frozen input sources differ beyond qualified test/documentation changes".into(),
        );
    }
    let prefix = |s: &[u8]| -> Result<Vec<u8>> {
        let s = std::str::from_utf8(s)?;
        Ok(s.split_once("\n#[cfg(test)]\nmod tests")
            .ok_or("Checkpoint test boundary")?
            .0
            .as_bytes()
            .to_vec())
    };
    if prefix(old)? != prefix(new)? {
        return Err("Production checkpoint source differs".into());
    }
    Ok(())
}
pub(super) fn payload_generators(old: &[u8], new: &[u8]) -> Result<()> {
    let normalize = |s: &[u8]| -> Result<String> {
        let s = std::str::from_utf8(s)?.replace("mod package_checks;\n", "");
        let (before, rest) = s
            .split_once("fn verify_apk() -> Result<()> {")
            .ok_or("Full package verifier boundary")?;
        let (_, after) = rest
            .split_once("fn walk(")
            .ok_or("Full package verifier end")?;
        Ok(format!("{before}fn walk({after}"))
    };
    if normalize(old)? != normalize(new)? {
        return Err("Full build guards or manifest/DEX/resource/notice generation changed".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn payload_gate_accepts_only_exact_replacement_marker_and_signatures() {
        let base = BTreeMap::from([
            (RUST_ENTRY.into(), "old".into()),
            (CAPABILITY.into(), "marker".into()),
            ("classes.dex".into(), "dex".into()),
            ("assets/notices/LICENSE".into(), "license".into()),
            ("META-INF/PSX-DEVE.SF".into(), "oldsign".into()),
        ]);
        let mut good = base.clone();
        good.remove(CAPABILITY);
        good.insert(RUST_ENTRY.into(), "normal".into());
        good.insert("META-INF/PSX-DEVE.SF".into(), "newsign".into());
        assert!(permitted(&base, &good, "normal").is_ok());
        for key in ["classes.dex", "assets/notices/LICENSE", RUST_ENTRY] {
            let mut bad = good.clone();
            bad.insert(key.into(), "changed".into());
            assert!(permitted(&base, &bad, "normal").is_err());
        }
        let mut bad = good.clone();
        bad.insert(CAPABILITY.into(), "marker".into());
        assert!(permitted(&base, &bad, "normal").is_err());
        let mut bad = good;
        bad.insert("META-INF/OTHER.RSA".into(), "unknown".into());
        assert!(permitted(&base, &bad, "normal").is_err());
    }
    #[test]
    fn zip_listing_rejects_duplicates_escapes_absolute_control_and_empty() {
        assert!(names("classes.dex\nlib/arm64-v8a/libpsx.so\n").is_ok());
        for bad in ["", "a\na\n", "../a\n", "/a\n", "a\\b\n", "a\tbad\n"] {
            assert!(names(bad).is_err());
        }
    }
    #[test]
    fn source_gate_distinguishes_tests_from_production_and_full_guard_changes() {
        let delta = b"crates/player/src/resume.rs\ndocs/architecture.md\n";
        let a = b"production\n#[cfg(test)]\nmod tests {old}";
        let b = b"production\n#[cfg(test)]\nmod tests {new}";
        assert!(coherence(delta, a, b).is_ok());
        assert!(coherence(delta, a, b"changed\n#[cfg(test)]\nmod tests {}").is_err());
        assert!(coherence(b"android/AndroidManifest.xml\n", a, b).is_err());
        let old = b"guard8;\nfn verify_apk() -> Result<()> {old}\nfn walk(rest";
        let new =
            b"mod package_checks;\nguard8;\nfn verify_apk() -> Result<()> {new}\nfn walk(rest";
        assert!(payload_generators(old, new).is_ok());
        assert!(payload_generators(
            old,
            b"guard2;\nfn verify_apk() -> Result<()> {new}\nfn walk(rest"
        )
        .is_err());
    }
    #[test]
    fn signer_requires_one_expected_certificate() {
        assert!(signer(
            "Signer #1 certificate SHA-256 digest: expected\n",
            "expected"
        )
        .is_ok());
        for bad in ["","Signer #1 certificate SHA-256 digest: wrong\n","Signer #1 certificate SHA-256 digest: expected\nSigner #2 certificate SHA-256 digest: other"] {assert!(signer(bad,"expected").is_err());}
    }
}
