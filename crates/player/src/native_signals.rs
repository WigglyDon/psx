//! A native runtime owns its process fault handlers until its workers retire.
//! Keep the inherited actions intact, including masks and platform flags.
#[cfg(any(target_os = "linux", target_os = "android"))]
mod platform {
    use std::{ffi::c_void, io, mem::MaybeUninit, ptr};

    const SIGNALS: [libc::c_int; 2] = [libc::SIGSEGV, libc::SIGBUS];
    #[cfg(feature = "gs-replay")]
    const RUNNER_SIGNALS: [libc::c_int; 2] = [libc::SIGINT, libc::SIGTERM];

    pub(crate) struct NativeSignals {
        owner: usize,
        previous: [libc::sigaction; 2],
        #[cfg(feature = "gs-replay")]
        runner_previous: Option<[libc::sigaction; 2]>,
    }

    fn module(address: *const c_void) -> Option<usize> {
        let mut info = MaybeUninit::<libc::Dl_info>::uninit();
        // dladdr searches loaded mappings; it does not call/dereference address.
        if unsafe { libc::dladdr(address, info.as_mut_ptr()) } == 0 {
            None
        } else {
            Some(unsafe { info.assume_init() }.dli_fbase as usize)
        }
    }

    fn action(signal: libc::c_int) -> io::Result<libc::sigaction> {
        let mut action = MaybeUninit::uninit();
        if unsafe { libc::sigaction(signal, ptr::null(), action.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { action.assume_init() })
    }

    impl NativeSignals {
        /// Capture before native initialization, while the sole core lease is held.
        pub(crate) fn capture(owner: *const c_void) -> io::Result<Self> {
            Ok(Self {
                owner: module(owner)
                    .ok_or_else(|| io::Error::other("Native engine mapping is unavailable."))?,
                previous: [action(SIGNALS[0])?, action(SIGNALS[1])?],
                #[cfg(feature = "gs-replay")]
                runner_previous: None,
            })
        }
        /// The retained CLI runner also installs interrupt/termination handlers.
        /// Only this new development entry extends the original two-signal scope.
        #[cfg(feature = "gs-replay")]
        pub(crate) fn capture_runner(owner: *const c_void) -> io::Result<Self> {
            let mut signals = Self::capture(owner)?;
            signals.runner_previous =
                Some([action(RUNNER_SIGNALS[0])?, action(RUNNER_SIGNALS[1])?]);
            Ok(signals)
        }

        /// Call after native workers/deinit and before releasing their library.
        /// A handler installed by another module is not ours to replace.
        pub(crate) fn restore(&self) -> io::Result<usize> {
            let mut restored = 0;
            for (signal, previous) in SIGNALS.into_iter().zip(&self.previous) {
                let current = action(signal)?;
                if module(current.sa_sigaction as *const c_void) == Some(self.owner) {
                    if unsafe { libc::sigaction(signal, previous, ptr::null_mut()) } != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    restored += 1;
                }
            }
            #[cfg(feature = "gs-replay")]
            if let Some(previous) = &self.runner_previous {
                for (signal, previous) in RUNNER_SIGNALS.into_iter().zip(previous) {
                    let current = action(signal)?;
                    if module(current.sa_sigaction as *const c_void) == Some(self.owner) {
                        if unsafe { libc::sigaction(signal, previous, ptr::null_mut()) } != 0 {
                            return Err(io::Error::last_os_error());
                        }
                        restored += 1;
                    }
                }
            }
            Ok(restored)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[cfg(feature = "gs-replay")]
        #[test]
        fn runner_interrupt_handlers_retire_after_join_with_foreign_actions_preserved() {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "native_signals::platform::tests::runner_signal_fixture",
                    "--nocapture",
                ])
                .env("PSX_RUNNER_SIGNAL_FIXTURE", "1")
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        #[cfg(feature = "gs-replay")]
        #[test]
        #[ignore = "isolated signal-owner fixture; parent executes it"]
        fn runner_signal_fixture() {
            assert_eq!(
                std::env::var("PSX_RUNNER_SIGNAL_FIXTURE").as_deref(),
                Ok("1")
            );
            let signals = [SIGNALS[0], SIGNALS[1], RUNNER_SIGNALS[0], RUNNER_SIGNALS[1]];
            let before: Vec<_> = signals.iter().map(|s| action(*s).unwrap()).collect();
            let install = |signal, action: &libc::sigaction| {
                assert_eq!(
                    unsafe { libc::sigaction(signal, action, ptr::null_mut()) },
                    0
                )
            };
            let mut baseline: libc::sigaction = unsafe { std::mem::zeroed() };
            baseline.sa_sigaction = libc::SIG_IGN;
            baseline.sa_flags = libc::SA_RESTART;
            unsafe {
                libc::sigemptyset(&mut baseline.sa_mask);
                libc::sigaddset(&mut baseline.sa_mask, libc::SIGUSR1);
            }
            for signal in signals {
                install(signal, &baseline);
            }
            let owner = libc::abort as *const () as *const c_void;
            let scope = NativeSignals::capture_runner(owner).unwrap();
            let mut owned = baseline;
            owned.sa_sigaction = owner as usize;
            for signal in signals {
                install(signal, &owned);
            }
            assert_eq!(scope.restore().unwrap(), 4);
            for signal in signals {
                let a = action(signal).unwrap();
                assert_eq!(a.sa_sigaction, libc::SIG_IGN);
                assert_ne!(a.sa_flags & libc::SA_RESTART, 0);
                assert_eq!(unsafe { libc::sigismember(&a.sa_mask, libc::SIGUSR1) }, 1);
            }
            for signal in signals {
                install(signal, &owned);
            }
            let mut foreign = baseline;
            foreign.sa_sigaction = libc::SIG_DFL;
            install(libc::SIGTERM, &foreign);
            assert_eq!(scope.restore().unwrap(), 3);
            assert_eq!(action(libc::SIGTERM).unwrap().sa_sigaction, libc::SIG_DFL);
            for (signal, original) in signals.into_iter().zip(&before) {
                install(signal, original);
            }
        }

        #[test]
        fn native_handlers_retire_with_masks_and_foreign_owners_preserved() {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "native_signals::platform::tests::signal_fixture",
                    "--nocapture",
                ])
                .env("PSX_SIGNAL_FIXTURE", "1")
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
        }

        // All process-wide mutations are confined to the parent's child.
        #[test]
        #[ignore]
        fn signal_fixture() {
            assert_eq!(std::env::var("PSX_SIGNAL_FIXTURE").as_deref(), Ok("1"));
            let before = [action(SIGNALS[0]).unwrap(), action(SIGNALS[1]).unwrap()];
            let install = |signal, action: &libc::sigaction| {
                assert_eq!(
                    unsafe { libc::sigaction(signal, action, ptr::null_mut()) },
                    0
                );
            };
            // A real DSO address lets the production dladdr ownership check run.
            // This handler is never invoked; no fault is deliberately raised.
            let owner = libc::abort as *const () as *const c_void;
            let mut baseline: libc::sigaction = unsafe { std::mem::zeroed() };
            baseline.sa_sigaction = libc::SIG_IGN;
            baseline.sa_flags = libc::SA_RESTART;
            unsafe {
                libc::sigemptyset(&mut baseline.sa_mask);
                libc::sigaddset(&mut baseline.sa_mask, libc::SIGUSR1);
            }
            for signal in SIGNALS {
                install(signal, &baseline);
            }
            for _ in 0..2 {
                let scope = NativeSignals::capture(owner).unwrap();
                let mut owned = baseline;
                owned.sa_sigaction = owner as usize;
                for signal in SIGNALS {
                    install(signal, &owned);
                }
                assert_eq!(scope.restore().unwrap(), 2);
                for signal in SIGNALS {
                    let restored = action(signal).unwrap();
                    assert_eq!(restored.sa_sigaction, baseline.sa_sigaction);
                    assert_eq!(restored.sa_flags, action(SIGNALS[0]).unwrap().sa_flags);
                    assert_ne!(restored.sa_flags & libc::SA_RESTART, 0);
                    assert_eq!(
                        unsafe { libc::sigismember(&restored.sa_mask, libc::SIGUSR1) },
                        1
                    );
                }
            }
            let scope = NativeSignals::capture(owner).unwrap();
            let mut owned = baseline;
            owned.sa_sigaction = owner as usize;
            install(libc::SIGSEGV, &owned);
            let mut foreign = baseline;
            foreign.sa_sigaction = libc::SIG_DFL;
            foreign.sa_flags = 0;
            install(libc::SIGBUS, &foreign);
            assert_eq!(scope.restore().unwrap(), 1);
            assert_eq!(action(libc::SIGBUS).unwrap().sa_sigaction, libc::SIG_DFL);
            for (signal, original) in SIGNALS.into_iter().zip(&before) {
                install(signal, original);
            }
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
mod platform {
    pub(crate) struct NativeSignals;
    impl NativeSignals {
        pub(crate) fn capture(_: *const std::ffi::c_void) -> std::io::Result<Self> {
            Ok(Self)
        }
        #[cfg(feature = "gs-replay")]
        pub(crate) fn capture_runner(owner: *const std::ffi::c_void) -> std::io::Result<Self> {
            Self::capture(owner)
        }
        pub(crate) fn restore(&self) -> std::io::Result<usize> {
            Ok(0)
        }
    }
}

pub(crate) use platform::NativeSignals;
