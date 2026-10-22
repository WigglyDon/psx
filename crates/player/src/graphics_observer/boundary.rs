//! Transient native/Rust boundary. Addresses stay on this thread and never
//! enter a recorder, export, log, or durable game identity.
use std::{cell::RefCell, ffi::c_void};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dispatch {
    pub version: u32,
    pub size: u32,
    pub context: usize,
    pub create: usize,
    pub valid: usize,
    pub status: usize,
    pub error: usize,
}

pub type Callback =
    unsafe extern "C" fn(u32, u32, u64, u32, u32, u32, *mut c_void, *const Dispatch) -> u64;

/// Status is queried only when both dispatches recognize the object.
pub fn classify(
    present: bool,
    native_valid: impl FnOnce() -> bool,
    rust_valid: impl FnOnce() -> bool,
    status: impl FnOnce() -> i64,
) -> (i64, i64, bool) {
    if !present {
        return (0, -2, false);
    }
    let native = native_valid();
    let rust = rust_valid();
    (
        if native && rust { status() } else { -1 },
        i64::from(native),
        native != rust,
    )
}

#[derive(Clone, Copy, Default)]
struct Pair {
    frame: u64,
    fence: usize,
    dispatch: Dispatch,
}
thread_local! {static PAIRS: RefCell<[Pair; 2]> = RefCell::new([Pair::default(); 2]);}

/// Same owning-thread pre/post identity, with no retained GL object ownership.
/// -2 = not a pair stage; 0 = pre-sample; 1 = matching post; -1 = mismatch.
pub fn pair(stage: u32, frame: u64, fence: usize, dispatch: Dispatch) -> i64 {
    let (index, pre) = match stage {
        16 => (0, true),
        2 => (0, false),
        17 => (1, true),
        7 => (1, false),
        _ => return -2,
    };
    PAIRS.with(|pairs| {
        let mut pairs = pairs.borrow_mut();
        if pre {
            pairs[index] = Pair {
                frame,
                fence,
                dispatch,
            };
            0
        } else {
            let before = std::mem::take(&mut pairs[index]);
            if before.frame == frame && before.fence == fence && before.dispatch == dispatch {
                1
            } else {
                -1
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_and_null_never_query_status_unsignaled_is_valid() {
        assert_eq!(
            classify(false, || panic!(), || panic!(), || panic!()),
            (0, -2, false)
        );
        assert_eq!(
            classify(true, || false, || false, || panic!()),
            (-1, 0, false)
        );
        assert_eq!(
            classify(true, || false, || true, || panic!()),
            (-1, 0, true)
        );
        assert_eq!(
            classify(true, || true, || true, || glow::UNSIGNALED as i64),
            (glow::UNSIGNALED as i64, 1, false)
        );
    }
    #[test]
    fn layout_and_high_bit_handle_pairs_do_not_truncate_or_cross_owners() {
        assert_eq!(
            std::mem::size_of::<Dispatch>(),
            8 + 5 * std::mem::size_of::<usize>()
        );
        assert_eq!(std::mem::offset_of!(Dispatch, context), 8);
        let high = usize::MAX - 4096;
        let d = Dispatch {
            version: 2,
            size: std::mem::size_of::<Dispatch>() as u32,
            context: high,
            create: high - 1,
            valid: high - 2,
            status: high - 3,
            error: high - 4,
        };
        assert_eq!(pair(16, 7, high, d), 0);
        assert_eq!(pair(2, 7, high, d), 1);
        assert_eq!(pair(2, 7, high, d), -1);
        assert_eq!(pair(17, 8, high, d), 0);
        assert_eq!(pair(7, 8, high - 1, d), -1);
        assert_eq!(pair(16, 9, high, d), 0);
        assert_eq!(
            std::thread::spawn(move || pair(2, 9, high, d))
                .join()
                .unwrap(),
            -1
        );
    }
}
