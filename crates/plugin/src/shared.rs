//! Memory shared between Celesta and a plugin's process, which large
//! payloads (a codec's decoded video frames) can cross instead of the pipe.
//!
//! Celesta creates a segment ([`SharedMemory::create`]) before it starts the
//! plugin, hands it to the plugin's process ([`SharedMemory::prepare`], then
//! [`SharedMemory::handle_for`]), and names that handle in its `hello`; the
//! plugin maps it ([`SharedMemory::open`]). The segment has no name anyone
//! else can open it by, its size is fixed once made, and it lives until both
//! sides have let go of it:
//!
//! - Linux: a `memfd`, sealed against growing and shrinking.
//! - Other Unix (macOS): a POSIX shared memory object, mode `0600`, unlinked
//!   as soon as it is made (whose size cannot change after its first
//!   `ftruncate`).
//! - On Unix the plugin inherits the descriptor at its own number, with
//!   close-on-exec cleared in the plugin's process alone, and sets it again
//!   once mapped, so processes the plugin starts do not inherit it.
//! - Windows: an unnamed pagefile-backed section, duplicated into the
//!   plugin's process after it starts.
//!
//! Neither side ever holds a Rust reference into the segment: bytes are
//! copied in and out through raw pointers, and the slot header's generation
//! is an atomic, so what the other process does to it cannot break this
//! process's aliasing rules. The protocol decides who writes when (see
//! `docs/handoff/2026-11-08-codec-shared-memory.md`).

use std::io;
use std::sync::atomic::{AtomicU64, Ordering, fence};

use serde::{Deserialize, Serialize};

/// The bytes a slot's header takes before its data: its generation (an
/// atomic `u64`) and its length, padded to a cache line.
pub const SLOT_HEADER: usize = 64;

/// A mapped segment.
pub struct SharedMemory {
    ptr: *mut u8,
    len: usize,
    handle: os::Handle,
}

// SAFETY: the mapping belongs to this value, which only copies through raw
// pointers and atomics, never handing out references into it.
unsafe impl Send for SharedMemory {}
// SAFETY: as above; every access goes through `&self` methods that copy or
// use atomics.
unsafe impl Sync for SharedMemory {}

impl SharedMemory {
    /// A new segment of `len` bytes, all zero, for a plugin process this one
    /// is about to start.
    pub fn create(len: usize) -> io::Result<Self> {
        if len <= SLOT_HEADER {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a shared segment holds at least one slot's header and a byte",
            ));
        }
        let (handle, ptr) = os::create(len)?;
        Ok(Self { ptr, len, handle })
    }

    /// Maps the segment of `len` bytes that `handle` (a descriptor on Unix,
    /// a `HANDLE` on Windows, as the `hello` request names it) refers to.
    /// Fails when the segment is smaller than `len`. The handle becomes this
    /// value's, and is closed with it; on Unix it is made close-on-exec, so
    /// processes this one starts do not inherit the segment.
    pub fn open(handle: u64, len: usize) -> io::Result<Self> {
        if len <= SLOT_HEADER {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a shared segment holds at least one slot's header and a byte",
            ));
        }
        let (handle, ptr) = os::open(handle, len)?;
        Ok(Self { ptr, len, handle })
    }

    /// The segment's length in bytes.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Always false: a segment holds at least a header and a byte.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// The bytes a slot at the start of the segment holds after its header.
    pub fn slot_bytes(&self) -> usize {
        self.len - SLOT_HEADER
    }

    /// Makes `command` pass the segment on to the process it starts: on
    /// Unix, its descriptor, kept open across `exec` in that process alone;
    /// on Windows, nothing (see [`Self::handle_for`]).
    pub fn prepare(&self, command: &mut std::process::Command) {
        os::prepare(&self.handle, command);
    }

    /// The handle the process `child`, started from a command
    /// [`Self::prepare`]d with this segment, maps it by: the descriptor's
    /// number on Unix (the same in the child), a handle duplicated into it on
    /// Windows.
    pub fn handle_for(&self, child: &std::process::Child) -> io::Result<u64> {
        os::handle_for(&self.handle, child)
    }

    /// Copies `bytes` into the slot's data under `generation`, as a
    /// seqlock's writer: it stores the odd generation before it (the slot is
    /// being written) with relaxed ordering, issues a release fence so the
    /// data cannot become visible before that, copies, then stores
    /// `generation` with release ordering, so a reader that loads it sees the
    /// bytes. `generation` must be even and not 0, and newer than the last
    /// one written. Fails, writing nothing, when they do not fit or
    /// `generation` is not even.
    pub fn write_slot(&self, bytes: &[u8], generation: u64) -> io::Result<()> {
        self.write_at(0, self.slot_bytes(), bytes, generation)
    }

    /// The slot's first `len` data bytes, copied out, as a seqlock's reader:
    /// only if its generation is `generation` (so not odd, being written)
    /// when loaded with acquire ordering before the copy, and still after it
    /// behind an acquire fence, so the bytes are the ones written under it
    /// and nothing wrote over them meanwhile; `None` otherwise, or when `len`
    /// is more than the slot holds or than was written. `len` and the slot's
    /// size are the caller's and this value's own: the length in the shared
    /// header is only compared with `len`.
    pub fn read_slot(&self, len: usize, generation: u64) -> Option<Vec<u8>> {
        self.read_at(0, self.slot_bytes(), len, generation)
    }

    /// The segment as a ring of `layout.slots` slots. Fails when the layout
    /// has no slots or no bytes in them, or its segment is longer than this
    /// one.
    pub fn ring(&self, layout: RingLayout) -> io::Result<Ring<'_>> {
        if layout.slots == 0 || layout.slot_bytes == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a ring has at least one slot of at least one byte",
            ));
        }
        if layout.slots > 1 && !layout.slot_bytes.is_multiple_of(8) {
            // Each slot's generation is an atomic `u64`, which must be
            // aligned: the stride must keep every slot's header 8-aligned.
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "slots of {} bytes would misalign the next slot's header",
                    layout.slot_bytes
                ),
            ));
        }
        let fits = layout
            .slots
            .checked_mul(layout.stride())
            .is_some_and(|bytes| bytes <= self.len);
        if !fits {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "a ring of {} slots of {} bytes does not fit a segment of {}",
                    layout.slots, layout.slot_bytes, self.len
                ),
            ));
        }
        Ok(Ring {
            memory: self,
            layout,
        })
    }

    /// The seqlock writer for the slot at `offset`, whose data holds
    /// `capacity` bytes; see [`Self::write_slot`].
    fn write_at(
        &self,
        offset: usize,
        capacity: usize,
        bytes: &[u8],
        generation: u64,
    ) -> io::Result<()> {
        if generation == 0 || !generation.is_multiple_of(2) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("a slot's generation is even and not 0, not {generation}"),
            ));
        }
        if bytes.len() > capacity {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} bytes do not fit a slot of {capacity}", bytes.len()),
            ));
        }
        self.generation_at(offset)
            .store(generation - 1, Ordering::Relaxed);
        fence(Ordering::Release);
        // SAFETY: the slot at `offset` lies within the mapping (its callers
        // check), its data starts after the header, and `bytes` fits it,
        // checked above; nothing in this process references the mapping.
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                self.ptr.add(offset + SLOT_HEADER),
                bytes.len(),
            );
        }
        self.length_at(offset)
            .store(bytes.len() as u64, Ordering::Relaxed);
        self.generation_at(offset)
            .store(generation, Ordering::Release);
        Ok(())
    }

    /// The seqlock reader for the slot at `offset`, whose data holds
    /// `capacity` bytes; see [`Self::read_slot`].
    fn read_at(
        &self,
        offset: usize,
        capacity: usize,
        len: usize,
        generation: u64,
    ) -> Option<Vec<u8>> {
        if generation % 2 == 1 || len > capacity {
            return None;
        }
        if self.generation_at(offset).load(Ordering::Acquire) != generation
            || self.length_at(offset).load(Ordering::Relaxed) != len as u64
        {
            return None;
        }
        let mut bytes = Vec::with_capacity(len);
        // SAFETY: the slot at `offset` lies within the mapping (its callers
        // check) and its data, after the header, holds `len` bytes, checked
        // above; `bytes` has room for them. A writer breaking the protocol
        // could be changing them meanwhile, which the second load below
        // catches; they are copied through raw pointers, never read through
        // a reference.
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.ptr.add(offset + SLOT_HEADER),
                bytes.as_mut_ptr(),
                len,
            );
            bytes.set_len(len);
        }
        fence(Ordering::Acquire);
        (self.generation_at(offset).load(Ordering::Relaxed) == generation).then_some(bytes)
    }

    fn generation_at(&self, offset: usize) -> &AtomicU64 {
        // SAFETY: `offset` is a slot's, within the mapping and 8-aligned
        // (the mapping is page-aligned, and `ring` refuses strides that
        // are not a multiple of 8 for more than one slot), so the first 8
        // bytes of its header are an aligned `u64` that lives as long as
        // `self`; both processes only touch it atomically.
        unsafe { AtomicU64::from_ptr(self.ptr.add(offset).cast()) }
    }

    fn length_at(&self, offset: usize) -> &AtomicU64 {
        // SAFETY: as `generation_at`, 8 bytes further.
        unsafe { AtomicU64::from_ptr(self.ptr.add(offset + 8).cast()) }
    }

    #[cfg(test)]
    fn slot_generation(&self) -> &AtomicU64 {
        self.generation_at(0)
    }
}

/// How a segment divides into slots: `slots` of them, each a
/// [`SLOT_HEADER`] and `slot_bytes` of data, one after another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RingLayout {
    pub slots: usize,
    pub slot_bytes: usize,
}

impl RingLayout {
    /// The bytes one slot takes, header and data.
    pub const fn stride(&self) -> usize {
        SLOT_HEADER + self.slot_bytes
    }

    /// The bytes the whole ring takes.
    pub const fn segment_bytes(&self) -> usize {
        self.slots * self.stride()
    }

    /// Where slot `slot` starts in the segment.
    pub const fn offset(&self, slot: usize) -> usize {
        slot * self.stride()
    }
}

/// A segment seen as a [`RingLayout`]'s slots, each its own seqlock.
pub struct Ring<'a> {
    memory: &'a SharedMemory,
    layout: RingLayout,
}

impl Ring<'_> {
    pub fn layout(&self) -> RingLayout {
        self.layout
    }

    /// [`SharedMemory::write_slot`] for slot `slot`; fails, writing nothing,
    /// when there is no such slot.
    pub fn write_slot(&self, slot: usize, bytes: &[u8], generation: u64) -> io::Result<()> {
        if slot >= self.layout.slots {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("slot {slot} of a ring of {}", self.layout.slots),
            ));
        }
        self.memory.write_at(
            self.layout.offset(slot),
            self.layout.slot_bytes,
            bytes,
            generation,
        )
    }

    /// [`SharedMemory::read_slot`] for slot `slot`; `None` when there is no
    /// such slot.
    pub fn read_slot(&self, slot: usize, len: usize, generation: u64) -> Option<Vec<u8>> {
        if slot >= self.layout.slots {
            return None;
        }
        self.memory.read_at(
            self.layout.offset(slot),
            self.layout.slot_bytes,
            len,
            generation,
        )
    }

    #[cfg(test)]
    fn slot_generation(&self, slot: usize) -> &AtomicU64 {
        self.memory.generation_at(self.layout.offset(slot))
    }
}

impl Drop for SharedMemory {
    fn drop(&mut self) {
        os::close(&self.handle, self.ptr, self.len);
    }
}

#[cfg(unix)]
mod os {
    use std::io;
    use std::os::unix::process::CommandExt;

    pub type Handle = libc::c_int;

    fn last_error() -> io::Error {
        io::Error::last_os_error()
    }

    /// Closes `fd` when dropped, until `into_inner`.
    struct Fd(libc::c_int);

    impl Fd {
        fn into_inner(self) -> libc::c_int {
            let fd = self.0;
            std::mem::forget(self);
            fd
        }
    }

    impl Drop for Fd {
        fn drop(&mut self) {
            // SAFETY: the descriptor is this value's.
            unsafe { libc::close(self.0) };
        }
    }

    #[cfg(target_os = "linux")]
    fn new_fd(len: usize) -> io::Result<Fd> {
        // SAFETY: the name is a valid C string; the flags are memfd's.
        let fd = unsafe {
            libc::memfd_create(
                c"celesta-shared".as_ptr(),
                libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
            )
        };
        if fd < 0 {
            return Err(last_error());
        }
        let fd = Fd(fd);
        // SAFETY: `fd` is a memfd this function owns.
        if unsafe { libc::ftruncate(fd.0, len as libc::off_t) } != 0 {
            return Err(last_error());
        }
        let seals = libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_SEAL;
        // SAFETY: as above.
        if unsafe { libc::fcntl(fd.0, libc::F_ADD_SEALS, seals) } != 0 {
            return Err(last_error());
        }
        Ok(fd)
    }

    #[cfg(not(target_os = "linux"))]
    fn new_fd(len: usize) -> io::Result<Fd> {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.subsec_nanos());
        // At most 31 bytes with its slash (macOS's `PSHMNAMLEN`).
        let name = format!(
            "/celesta-{:x}-{:x}-{:x}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            nanos
        );
        let name = std::ffi::CString::new(name).expect("no NUL in the name");
        // SAFETY: `name` is a valid C string; the mode is passed as the
        // variadic argument `shm_open` reads for `O_CREAT`.
        let fd = unsafe {
            libc::shm_open(
                name.as_ptr(),
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
                0o600 as libc::c_uint,
            )
        };
        if fd < 0 {
            return Err(last_error());
        }
        let fd = Fd(fd);
        // Nameless from here on: only descriptors reach it.
        // SAFETY: `name` is the object just made.
        unsafe { libc::shm_unlink(name.as_ptr()) };
        // SAFETY: `fd` is this function's.
        unsafe { libc::fcntl(fd.0, libc::F_SETFD, libc::FD_CLOEXEC) };
        // SAFETY: as above.
        if unsafe { libc::ftruncate(fd.0, len as libc::off_t) } != 0 {
            return Err(last_error());
        }
        Ok(fd)
    }

    fn map(fd: &Fd, len: usize) -> io::Result<*mut u8> {
        // SAFETY: a shared read-write mapping of `len` bytes of `fd`, whose
        // size the caller has made or checked to be at least `len`.
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd.0,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(last_error());
        }
        Ok(ptr.cast())
    }

    pub fn create(len: usize) -> io::Result<(Handle, *mut u8)> {
        let fd = new_fd(len)?;
        let ptr = map(&fd, len)?;
        Ok((fd.into_inner(), ptr))
    }

    pub fn open(handle: u64, len: usize) -> io::Result<(Handle, *mut u8)> {
        let fd = libc::c_int::try_from(handle)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "not a descriptor"))?;
        let fd = Fd(fd);
        // SAFETY: `stat` is written by `fstat` before it is read.
        let mut stat = unsafe { std::mem::zeroed::<libc::stat>() };
        // SAFETY: as above.
        if unsafe { libc::fstat(fd.0, &mut stat) } != 0 {
            return Err(last_error());
        }
        if (stat.st_size as u64) < len as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("the segment has {} bytes, not {len}", stat.st_size),
            ));
        }
        let ptr = map(&fd, len)?;
        // Inherited without close-on-exec: set it, so processes this one
        // starts do not inherit the segment too.
        // SAFETY: `fd` is this function's.
        unsafe { libc::fcntl(fd.0, libc::F_SETFD, libc::FD_CLOEXEC) };
        Ok((fd.into_inner(), ptr))
    }

    pub fn prepare(handle: &Handle, command: &mut std::process::Command) {
        let fd = *handle;
        // The descriptor keeps its own number: moving it to a fixed one
        // could close one std still uses after `pre_exec` (the pipe it
        // reports a failed `exec` through), and `dup2` onto its own number
        // would leave close-on-exec set. Only the child's copy is cleared,
        // so processes other threads start meanwhile do not inherit it.
        // SAFETY: `fcntl` is async-signal-safe, as `pre_exec` requires, and
        // touches only the child's descriptor.
        unsafe {
            command.pre_exec(move || {
                if libc::fcntl(fd, libc::F_SETFD, 0) != 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    pub fn handle_for(handle: &Handle, _: &std::process::Child) -> io::Result<u64> {
        Ok(*handle as u64)
    }

    pub fn close(handle: &Handle, ptr: *mut u8, len: usize) {
        // SAFETY: `ptr` is the mapping of `len` bytes this value made, and
        // `handle` its descriptor; neither is used again.
        unsafe {
            libc::munmap(ptr.cast(), len);
            libc::close(*handle);
        }
    }

    pub fn duplicate(handle: &Handle) -> io::Result<u64> {
        // SAFETY: duplicates a descriptor this process holds.
        let fd = unsafe { libc::fcntl(*handle, libc::F_DUPFD_CLOEXEC, 0) };
        if fd < 0 {
            return Err(last_error());
        }
        Ok(fd as u64)
    }
}

#[cfg(windows)]
mod os {
    use std::ffi::c_void;
    use std::io;
    use std::os::windows::io::AsRawHandle;

    pub type Handle = isize;

    const PAGE_READWRITE: u32 = 0x04;
    const FILE_MAP_READ: u32 = 0x04;
    const FILE_MAP_WRITE: u32 = 0x02;
    const DUPLICATE_SAME_ACCESS: u32 = 0x02;
    const INVALID_HANDLE_VALUE: isize = -1;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateFileMappingW(
            file: isize,
            attributes: *const c_void,
            protect: u32,
            maximum_size_high: u32,
            maximum_size_low: u32,
            name: *const u16,
        ) -> isize;
        fn MapViewOfFile(
            mapping: isize,
            access: u32,
            offset_high: u32,
            offset_low: u32,
            bytes: usize,
        ) -> *mut c_void;
        fn UnmapViewOfFile(address: *const c_void) -> i32;
        fn CloseHandle(handle: isize) -> i32;
        fn GetCurrentProcess() -> isize;
        fn DuplicateHandle(
            source_process: isize,
            source: isize,
            target_process: isize,
            target: *mut isize,
            access: u32,
            inherit: i32,
            options: u32,
        ) -> i32;
    }

    fn map(handle: isize, len: usize) -> io::Result<*mut u8> {
        // SAFETY: maps `len` bytes of a section this process holds a handle
        // to; it fails when the section is smaller.
        let ptr = unsafe { MapViewOfFile(handle, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, len) };
        if ptr.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(ptr.cast())
    }

    pub fn create(len: usize) -> io::Result<(Handle, *mut u8)> {
        let size = len as u64;
        // SAFETY: an unnamed pagefile-backed section of `len` bytes, with
        // the default security (no inheritance).
        let handle = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                std::ptr::null(),
                PAGE_READWRITE,
                (size >> 32) as u32,
                size as u32,
                std::ptr::null(),
            )
        };
        if handle == 0 {
            return Err(io::Error::last_os_error());
        }
        match map(handle, len) {
            Ok(ptr) => Ok((handle, ptr)),
            Err(error) => {
                // SAFETY: the handle was just made and is not used again.
                unsafe { CloseHandle(handle) };
                Err(error)
            }
        }
    }

    pub fn open(handle: u64, len: usize) -> io::Result<(Handle, *mut u8)> {
        let handle = isize::try_from(handle)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "not a handle"))?;
        match map(handle, len) {
            Ok(ptr) => Ok((handle, ptr)),
            Err(error) => {
                // SAFETY: the handle was given to this process to own.
                unsafe { CloseHandle(handle) };
                Err(error)
            }
        }
    }

    pub fn prepare(_: &Handle, _: &mut std::process::Command) {}

    pub fn handle_for(handle: &Handle, child: &std::process::Child) -> io::Result<u64> {
        let mut target = 0isize;
        // SAFETY: duplicates the section into the process `child`, whose
        // handle this process holds with every access as its creator.
        let duplicated = unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                *handle,
                child.as_raw_handle() as isize,
                &mut target,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        };
        if duplicated == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(target as u64)
    }

    pub fn close(handle: &Handle, ptr: *mut u8, _len: usize) {
        // SAFETY: `ptr` is the view this value mapped and `handle` its
        // section; neither is used again.
        unsafe {
            UnmapViewOfFile(ptr.cast());
            CloseHandle(*handle);
        }
    }

    pub fn duplicate(handle: &Handle) -> io::Result<u64> {
        let mut target = 0isize;
        // SAFETY: duplicates a handle this process holds into itself.
        let duplicated = unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                *handle,
                GetCurrentProcess(),
                &mut target,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        };
        if duplicated == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(target as u64)
    }
}

#[cfg(not(any(unix, windows)))]
mod os {
    use std::io;

    pub type Handle = ();

    fn unsupported() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "no shared memory on this platform",
        )
    }

    pub fn create(_: usize) -> io::Result<(Handle, *mut u8)> {
        Err(unsupported())
    }

    pub fn open(_: u64, _: usize) -> io::Result<(Handle, *mut u8)> {
        Err(unsupported())
    }

    pub fn prepare(_: &Handle, _: &mut std::process::Command) {}

    pub fn handle_for(_: &Handle, _: &std::process::Child) -> io::Result<u64> {
        Err(unsupported())
    }

    pub fn close(_: &Handle, _: *mut u8, _: usize) {}

    pub fn duplicate(_: &Handle) -> io::Result<u64> {
        Err(unsupported())
    }
}

impl SharedMemory {
    /// A second handle to the segment in this process, which
    /// [`Self::open`] can map it by (and then owns), as a plugin process
    /// would: for tests that play both sides in one process.
    pub fn duplicate_handle(&self) -> io::Result<u64> {
        os::duplicate(&self.handle)
    }
}

#[cfg(all(test, any(unix, windows)))]
mod tests {
    use super::*;

    #[test]
    fn what_one_side_writes_the_other_reads() {
        let host = SharedMemory::create(SLOT_HEADER + 1024).unwrap();
        let plugin = SharedMemory::open(host.duplicate_handle().unwrap(), host.len()).unwrap();
        assert_eq!(plugin.slot_bytes(), 1024);
        let bytes: Vec<u8> = (0..1000u32).map(|index| index as u8).collect();
        plugin.write_slot(&bytes, 8).unwrap();
        assert_eq!(host.read_slot(1000, 8), Some(bytes));
    }

    #[test]
    fn a_read_under_another_generation_or_length_is_refused() {
        let host = SharedMemory::create(SLOT_HEADER + 16).unwrap();
        let plugin = SharedMemory::open(host.duplicate_handle().unwrap(), host.len()).unwrap();
        plugin.write_slot(&[1, 2, 3, 4], 2).unwrap();
        assert_eq!(host.read_slot(4, 1), None, "an older generation");
        assert_eq!(host.read_slot(3, 2), None, "fewer bytes than written");
        assert_eq!(host.read_slot(17, 2), None, "more than the slot holds");
        assert_eq!(host.read_slot(4, 2), Some(vec![1, 2, 3, 4]));
    }

    #[test]
    fn bytes_that_do_not_fit_are_not_written() {
        let host = SharedMemory::create(SLOT_HEADER + 4).unwrap();
        host.write_slot(&[9; 4], 2).unwrap();
        assert!(host.write_slot(&[0; 5], 4).is_err());
        assert_eq!(host.read_slot(4, 2), Some(vec![9; 4]));
    }

    #[test]
    fn a_slot_being_written_is_not_read() {
        let host = SharedMemory::create(SLOT_HEADER + 4).unwrap();
        host.write_slot(&[1, 2, 3, 4], 2).unwrap();
        // What a writer leaves while it copies the next frame in.
        host.slot_generation().store(3, Ordering::Relaxed);
        assert_eq!(host.read_slot(4, 2), None, "the generation moved on");
        assert_eq!(host.read_slot(4, 3), None, "an odd generation is a write");
    }

    #[test]
    fn a_generation_that_is_odd_or_0_is_not_written() {
        let host = SharedMemory::create(SLOT_HEADER + 4).unwrap();
        assert!(host.write_slot(&[1], 3).is_err());
        assert!(host.write_slot(&[1], 0).is_err());
        assert_eq!(host.read_slot(0, 0), Some(Vec::new()), "nothing written");
    }

    #[test]
    fn a_segment_smaller_than_asked_for_is_not_mapped() {
        let host = SharedMemory::create(SLOT_HEADER + 4096).unwrap();
        let handle = host.duplicate_handle().unwrap();
        assert!(SharedMemory::open(handle, 1 << 30).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_mapped_descriptor_is_not_inherited_further() {
        let host = SharedMemory::create(SLOT_HEADER + 4).unwrap();
        // A descriptor without close-on-exec, as the plugin inherits it.
        // SAFETY: duplicates a descriptor this process holds.
        let inherited = unsafe { libc::dup(host.handle) };
        assert!(inherited >= 0);
        // SAFETY: as above.
        assert_eq!(
            unsafe { libc::fcntl(inherited, libc::F_GETFD) } & libc::FD_CLOEXEC,
            0
        );
        let plugin = SharedMemory::open(inherited as u64, host.len()).unwrap();
        // SAFETY: as above.
        let flags = unsafe { libc::fcntl(plugin.handle, libc::F_GETFD) };
        assert_ne!(flags & libc::FD_CLOEXEC, 0);
    }

    #[test]
    fn a_segment_starts_zeroed() {
        let host = SharedMemory::create(SLOT_HEADER + 8).unwrap();
        assert_eq!(host.read_slot(0, 0), Some(Vec::new()));
    }

    #[test]
    fn a_layout_measures_its_segment() {
        let layout = RingLayout {
            slots: 3,
            slot_bytes: 100,
        };
        assert_eq!(layout.stride(), 164);
        assert_eq!(layout.segment_bytes(), 492);
        assert_eq!(layout.offset(2), 328);
    }

    #[test]
    fn ring_slots_are_independent() {
        let layout = RingLayout {
            slots: 3,
            slot_bytes: 16,
        };
        let host = SharedMemory::create(layout.segment_bytes()).unwrap();
        let plugin = SharedMemory::open(host.duplicate_handle().unwrap(), host.len()).unwrap();
        let writer = plugin.ring(layout).unwrap();
        writer.write_slot(1, &[1; 8], 2).unwrap();
        writer.write_slot(2, &[2; 12], 4).unwrap();
        let reader = host.ring(layout).unwrap();
        assert_eq!(reader.read_slot(1, 8, 2), Some(vec![1; 8]));
        assert_eq!(reader.read_slot(2, 12, 4), Some(vec![2; 12]));
        assert_eq!(reader.read_slot(0, 8, 2), None);
        assert_eq!(reader.read_slot(1, 8, 4), None);
    }

    #[test]
    fn a_ring_larger_than_the_segment_is_refused() {
        let host = SharedMemory::create(SLOT_HEADER + 1024).unwrap();
        assert!(
            host.ring(RingLayout {
                slots: 2,
                slot_bytes: 1024
            })
            .is_err()
        );
        assert!(
            host.ring(RingLayout {
                slots: 0,
                slot_bytes: 1
            })
            .is_err()
        );
        assert!(
            host.ring(RingLayout {
                slots: 1,
                slot_bytes: 0
            })
            .is_err()
        );
        let misaligned = SharedMemory::create(3 * (SLOT_HEADER + 5)).unwrap();
        assert!(
            misaligned
                .ring(RingLayout {
                    slots: 3,
                    slot_bytes: 5
                })
                .is_err(),
            "the second header would not be aligned"
        );
        assert!(
            host.ring(RingLayout {
                slots: 1,
                slot_bytes: 1024
            })
            .is_ok()
        );
    }

    #[test]
    fn a_slot_past_the_ring_is_refused() {
        let layout = RingLayout {
            slots: 3,
            slot_bytes: 8,
        };
        let host = SharedMemory::create(layout.segment_bytes()).unwrap();
        let ring = host.ring(layout).unwrap();
        assert!(ring.write_slot(3, &[1], 2).is_err());
        assert_eq!(ring.read_slot(3, 1, 2), None);
    }

    #[test]
    fn the_one_slot_ring_is_the_old_slot() {
        let host = SharedMemory::create(SLOT_HEADER + 64).unwrap();
        let layout = RingLayout {
            slots: 1,
            slot_bytes: host.slot_bytes(),
        };
        let bytes: Vec<u8> = (0..40u8).collect();
        host.write_slot(&bytes, 8).unwrap();
        let ring = host.ring(layout).unwrap();
        assert_eq!(ring.read_slot(0, bytes.len(), 8), Some(bytes.clone()));
        let other: Vec<u8> = (50..70u8).collect();
        ring.write_slot(0, &other, 10).unwrap();
        assert_eq!(host.read_slot(other.len(), 10), Some(other));
    }

    #[test]
    fn a_slot_being_written_is_not_read_in_a_ring() {
        let layout = RingLayout {
            slots: 3,
            slot_bytes: 8,
        };
        let host = SharedMemory::create(layout.segment_bytes()).unwrap();
        let ring = host.ring(layout).unwrap();
        ring.write_slot(0, &[9; 4], 2).unwrap();
        ring.write_slot(1, &[1, 2, 3, 4], 2).unwrap();
        // What a writer leaves in slot 1 while it copies the next frame in.
        ring.slot_generation(1).store(3, Ordering::Relaxed);
        assert_eq!(ring.read_slot(1, 4, 2), None, "the generation moved on");
        assert_eq!(
            ring.read_slot(1, 4, 3),
            None,
            "an odd generation is a write"
        );
        assert_eq!(ring.read_slot(0, 4, 2), Some(vec![9; 4]));
    }
}
