//! Off-device bounded decoding through the existing libzstd ABI. No subprocess
//! prefetch or decoded file/tree; reject concatenated/trailing/incomplete frames.
use libloading::Library;
use std::{
    ffi::c_void,
    io::{self, Read},
};
#[repr(C)]
struct Input {
    src: *const c_void,
    size: usize,
    pos: usize,
}
#[repr(C)]
struct Output {
    dst: *mut c_void,
    size: usize,
    pos: usize,
}
type Decompress = unsafe extern "C" fn(*mut c_void, *mut Output, *mut Input) -> usize;
pub struct Decoder<R> {
    _library: Library,
    state: *mut c_void,
    decompress: Decompress,
    free: unsafe extern "C" fn(*mut c_void) -> usize,
    is_error: unsafe extern "C" fn(usize) -> u32,
    input: R,
    buffer: Box<[u8; 65536]>,
    start: usize,
    end: usize,
    decoded: u64,
    limit: u64,
    finished: bool,
    verified_end: bool,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
impl<R: Read> Decoder<R> {
    pub fn new(input: R, limit: u64) -> io::Result<Self> {
        if limit == 0 || limit > super::gs_replay::BYTE_LIMIT {
            return Err(invalid("invalid decoded bound"));
        }
        unsafe {
            let library = Library::new("libzstd.so.1").map_err(|e| invalid(&e.to_string()))?;
            macro_rules! symbol {
                ($name:literal,$ty:ty) => {
                    *library
                        .get::<$ty>(concat!($name, "\0").as_bytes())
                        .map_err(|e| invalid(&e.to_string()))?
                };
            }
            let create = symbol!("ZSTD_createDStream", unsafe extern "C" fn() -> *mut c_void);
            let free = symbol!(
                "ZSTD_freeDStream",
                unsafe extern "C" fn(*mut c_void) -> usize
            );
            let init = symbol!(
                "ZSTD_initDStream",
                unsafe extern "C" fn(*mut c_void) -> usize
            );
            let is_error = symbol!("ZSTD_isError", unsafe extern "C" fn(usize) -> u32);
            let parameter = symbol!(
                "ZSTD_DCtx_setParameter",
                unsafe extern "C" fn(*mut c_void, i32, i32) -> usize
            );
            let decompress = symbol!("ZSTD_decompressStream", Decompress);
            let state = create();
            if state.is_null() {
                return Err(invalid("zstd allocation refused"));
            }
            // ZSTD_d_windowLogMax=100 in the retained public ABI. The donor's
            // streaming level6 output fits this64MiB decoder-window ceiling.
            if is_error(init(state)) != 0 || is_error(parameter(state, 100, 26)) != 0 {
                free(state);
                return Err(invalid("zstd bounded initialization refused"));
            }
            Ok(Self {
                _library: library,
                state,
                decompress,
                free,
                is_error,
                input,
                buffer: Box::new([0; 65536]),
                start: 0,
                end: 0,
                decoded: 0,
                limit,
                finished: false,
                verified_end: false,
            })
        }
    }
    pub fn decoded_bytes(&self) -> u64 {
        self.decoded
    }
}
impl<R: Read> Read for Decoder<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        loop {
            if self.finished {
                if !self.verified_end {
                    let mut probe = [0];
                    if self.start != self.end || self.input.read(&mut probe)? != 0 {
                        return Err(invalid("trailing/concatenated zstd data"));
                    }
                    self.verified_end = true;
                }
                return Ok(0);
            }
            if self.start == self.end {
                self.end = self.input.read(&mut self.buffer[..])?;
                self.start = 0;
                if self.end == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "missing zstd trailer",
                    ));
                }
            }
            let mut input = Input {
                src: self.buffer.as_ptr().cast(),
                size: self.end,
                pos: self.start,
            };
            let remaining = self
                .limit
                .checked_sub(self.decoded)
                .ok_or_else(|| invalid("decoded limit"))?;
            let mut output = Output {
                dst: out.as_mut_ptr().cast(),
                size: out.len().min(remaining as usize),
                pos: 0,
            };
            let result = unsafe { (self.decompress)(self.state, &mut output, &mut input) };
            if unsafe { (self.is_error)(result) } != 0 {
                return Err(invalid("zstd decode failed"));
            }
            if input.pos > self.end || output.pos > output.size {
                return Err(invalid("zstd ABI extent"));
            }
            let progressed = input.pos != self.start || output.pos != 0;
            self.start = input.pos;
            self.decoded = self
                .decoded
                .checked_add(output.pos as u64)
                .ok_or_else(|| invalid("decoded overflow"))?;
            self.finished = result == 0;
            if output.pos != 0 {
                return Ok(output.pos);
            }
            if !progressed && !self.finished {
                return Err(invalid(if remaining == 0 {
                    "decoded limit"
                } else {
                    "stalled zstd stream"
                }));
            }
        }
    }
}
impl<R> Drop for Decoder<R> {
    fn drop(&mut self) {
        unsafe {
            (self.free)(self.state);
        }
    }
}
