//! Exact scratch-packet staging for the development-only renderer comparison.
//! This supplies command construction and completion checks, not device admission.
//! Use shell protocol v2 (`adb shell -T`): legacy `exec-in` does not collect
//! remote exit status or stderr. Input inventory/hash checks still follow.

#[derive(Clone, Copy)]
pub(crate) enum Packet {
    Reference,
    Control,
}

impl Packet {
    pub(crate) fn package(self) -> &'static str {
        match self {
            Self::Reference => "com.machinerhythm.psx.rendererreference",
            Self::Control => "com.machinerhythm.psx",
        }
    }

    fn root(self) -> &'static str {
        match self {
            Self::Reference => "reference-v1",
            Self::Control => "observer-scratch-v1",
        }
    }

    fn inventory(self) -> &'static str {
        match self {
            Self::Reference => "input-inventory.json",
            Self::Control => "library.json",
        }
    }

    pub(crate) fn script(self) -> String {
        let root = self.root();
        let inventory = self.inventory();
        format!(
            "test ! -L files && mkdir -p files && test -d files && \
             test ! -e files/{root} && test ! -L files/{root} && \
             tar -C files -xf - && test -d files/{root} && \
             test ! -L files/{root} && test -f files/{root}/{inventory} && \
             test ! -L files/{root}/{inventory} && \
             printf '%s\\n' PSX_PACKET_STAGED_V1"
        )
    }

    pub(crate) fn shell_command(self) -> String {
        // Every word comes from these two immutable modes, never a user path.
        let quoted = self.script().replace('\'', "'\\''");
        format!("run-as {} sh -c '{quoted}'", self.package())
    }
}

pub(crate) fn confirmed(exit: i32, stdout: &[u8], stderr: &[u8]) -> bool {
    exit == 0 && stdout == b"PSX_PACKET_STAGED_V1\n" && stderr.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        process::{Command, Output, Stdio},
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "psx-packet-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            fs::create_dir(root.join("app")).unwrap();
            fs::create_dir(root.join("source")).unwrap();
            Self(root)
        }
        fn packet(&self, packet: Packet, inventory: bool) -> PathBuf {
            let root = self.0.join("source").join(packet.root());
            fs::create_dir(&root).unwrap();
            if inventory {
                fs::write(root.join(packet.inventory()), b"{\"generated\":true}\n").unwrap();
            }
            fs::write(root.join("fixture.bin"), [0, 128, 255]).unwrap();
            let archive = self.0.join("packet.tar");
            assert!(Command::new("tar")
                .arg("-cf")
                .arg(&archive)
                .arg("-C")
                .arg(self.0.join("source"))
                .arg(packet.root())
                .status()
                .unwrap()
                .success());
            archive
        }
        fn stage(&self, packet: Packet, archive: &std::path::Path) -> Output {
            Command::new("sh")
                .args(["-c", &packet.script()])
                .current_dir(self.0.join("app"))
                .stdin(Stdio::from(fs::File::open(archive).unwrap()))
                .output()
                .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn fresh_app_root_is_prepared_before_extracting_both_exact_packets() {
        for packet in [Packet::Reference, Packet::Control] {
            let f = Fixture::new();
            let archive = f.packet(packet, true);
            // The old command has an unsatisfied destination prerequisite.
            let old = Command::new("tar")
                .args(["-xf", "-", "-C", "files"])
                .current_dir(f.0.join("app"))
                .stdin(Stdio::from(fs::File::open(&archive).unwrap()))
                .output()
                .unwrap();
            assert!(!old.status.success());
            let out = f.stage(packet, &archive);
            assert!(confirmed(
                out.status.code().unwrap(),
                &out.stdout,
                &out.stderr
            ));
            assert_eq!(
                fs::read(
                    f.0.join("app/files")
                        .join(packet.root())
                        .join("fixture.bin")
                )
                .unwrap(),
                [0, 128, 255]
            );
        }
    }

    #[test]
    fn absent_inventory_is_not_successful_staging() {
        let f = Fixture::new();
        let archive = f.packet(Packet::Reference, false);
        let out = f.stage(Packet::Reference, &archive);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
    }

    #[test]
    fn invalid_stream_preserves_extraction_error_and_never_emits_success() {
        let f = Fixture::new();
        let archive = f.0.join("bad.tar");
        fs::write(&archive, b"invalid generated archive").unwrap();
        let out = f.stage(Packet::Reference, &archive);
        assert!(!out.status.success());
        assert!(!out.stderr.is_empty());
        assert!(out.stdout.is_empty());
    }

    #[test]
    fn preexisting_scratch_is_preserved() {
        let f = Fixture::new();
        let archive = f.packet(Packet::Reference, true);
        let root = f.0.join("app/files/reference-v1");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("owner"), b"preserve").unwrap();
        let out = f.stage(Packet::Reference, &archive);
        assert!(!out.status.success());
        assert_eq!(fs::read(root.join("owner")).unwrap(), b"preserve");
        assert!(!root.join("fixture.bin").exists());
    }

    #[test]
    fn redirected_files_root_is_refused() {
        let f = Fixture::new();
        let archive = f.packet(Packet::Reference, true);
        let elsewhere = f.0.join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, f.0.join("app/files")).unwrap();
        let out = f.stage(Packet::Reference, &archive);
        assert!(!out.status.success());
        assert_eq!(fs::read_dir(elsewhere).unwrap().count(), 0);
    }

    #[test]
    fn success_requires_remote_status_exact_completion_and_no_hidden_error() {
        assert!(confirmed(0, b"PSX_PACKET_STAGED_V1\n", b""));
        assert!(!confirmed(0, b"", b""));
        assert!(!confirmed(1, b"PSX_PACKET_STAGED_V1\n", b""));
        assert!(!confirmed(0, b"PSX_PACKET_STAGED_V1\n", b"tar: failed\n"));
        assert!(!confirmed(0, b"noise\nPSX_PACKET_STAGED_V1\n", b""));
    }

    #[test]
    fn remote_command_is_one_quoted_script_for_two_fixed_packages_only() {
        for p in [Packet::Reference, Packet::Control] {
            let expected = format!("run-as {} sh -c '", p.package());
            assert!(p.shell_command().starts_with(&expected));
            assert!(p.shell_command().ends_with('\''));
            assert!(p.shell_command().contains("'\\''"));
        }
    }

    #[test]
    fn generated_remote_command_preserves_stdin_and_shell_quoting() {
        use std::os::unix::fs::PermissionsExt;
        for p in [Packet::Reference, Packet::Control] {
            let f = Fixture::new();
            let archive = f.packet(p, true);
            let bin = f.0.join("bin");
            fs::create_dir(&bin).unwrap();
            // Only a local fixture; never invokes Android or the real run-as.
            let shim = bin.join("run-as");
            fs::write(&shim, b"#!/bin/sh\ncase \"$1\" in com.machinerhythm.psx|com.machinerhythm.psx.rendererreference) shift; exec \"$@\";; *) exit 99;; esac\n").unwrap();
            fs::set_permissions(&shim, fs::Permissions::from_mode(0o700)).unwrap();
            let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
            let out = Command::new("sh")
                .args(["-c", &p.shell_command()])
                .env("PATH", path)
                .current_dir(f.0.join("app"))
                .stdin(Stdio::from(fs::File::open(archive).unwrap()))
                .output()
                .unwrap();
            assert!(confirmed(
                out.status.code().unwrap(),
                &out.stdout,
                &out.stderr
            ));
        }
    }
}
