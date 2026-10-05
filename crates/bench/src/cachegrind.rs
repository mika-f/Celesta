//! Cachegrind's `CACHEGRIND_START_INSTRUMENTATION` and
//! `CACHEGRIND_STOP_INSTRUMENTATION` client requests (Valgrind 3.22 and
//! later), so that with `--instr-at-start=no` only the measured frames are
//! counted: not process start, device creation, shader compiles, warmup, or
//! the React entry's evaluation.
//!
//! A client request is Valgrind's "special instruction" sequence from
//! `valgrind.h`: rotations that add up to a full turn, then a no-op
//! register move. Run natively it changes nothing; Valgrind recognizes it
//! and handles the request. Elsewhere these functions do nothing.

/// `VG_USERREQ_TOOL_BASE('C', 'G')`, the first request in `cachegrind.h`.
const START_INSTRUMENTATION: u64 = (b'C' as u64) << 24 | (b'G' as u64) << 16;
const STOP_INSTRUMENTATION: u64 = START_INSTRUMENTATION + 1;

pub fn start() {
    request(START_INSTRUMENTATION);
}

pub fn stop() {
    request(STOP_INSTRUMENTATION);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn request(code: u64) {
    let args = [code, 0, 0, 0, 0, 0];
    // SAFETY: `rdi` turns 3 + 13 + 61 + 51 = 128 bits, back to its value;
    // `xchg rbx, rbx` changes nothing. Valgrind reads `args` through `rax`
    // and writes its result to `rdx`.
    unsafe {
        std::arch::asm!(
            "rol rdi, 3",
            "rol rdi, 13",
            "rol rdi, 61",
            "rol rdi, 51",
            "xchg rbx, rbx",
            in("rax") args.as_ptr(),
            inout("rdx") 0u64 => _,
            options(nostack),
        );
    }
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
fn request(code: u64) {
    let args = [code, 0, 0, 0, 0, 0];
    // SAFETY: `x12` turns 3 + 13 + 51 + 61 = 128 bits, back to its value;
    // `orr x10, x10, x10` changes nothing. Valgrind reads `args` through
    // `x4` and writes its result to `x3`.
    unsafe {
        std::arch::asm!(
            "ror x12, x12, #3",
            "ror x12, x12, #13",
            "ror x12, x12, #51",
            "ror x12, x12, #61",
            "orr x10, x10, x10",
            in("x4") args.as_ptr(),
            inout("x3") 0u64 => _,
            options(nostack),
        );
    }
}

#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
fn request(_code: u64) {}
