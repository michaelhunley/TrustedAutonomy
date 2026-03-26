//! Copy strategy detection and COW-aware file copying for overlay workspaces.
//!
//! Detects whether the target filesystem supports APFS clone (macOS) or Btrfs
//! reflink (Linux) and selects the most efficient copy method. Falls back to
//! regular `fs::copy` on unsupported filesystems.
//!
//! ## How it works
//!
//! At workspace creation time, [`detect_strategy`] probes the staging directory
//! by creating a tiny temp file and attempting to clone it. If the clone
//! succeeds, subsequent file copies use the COW method — the kernel shares the
//! data pages between source and copy until one of them writes. The staging copy
//! is effectively instantaneous and consumes no additional disk space until the
//! agent actually modifies a file.
//!
//! If the probe fails (filesystem doesn't support COW, cross-device copy, etc.),
//! the fallback is the current byte-for-byte `fs::copy` path (V1 behavior).

use std::io;
use std::path::Path;
use std::time::Duration;

/// The copy method used when creating the staging workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyStrategy {
    /// Standard byte-for-byte copy via `fs::copy` (always supported).
    Full,
    /// APFS clone via `clonefile(2)` syscall (macOS 10.12+, zero-cost until write).
    ApfsClone,
    /// Btrfs reflink via `FICLONE` ioctl (Linux, zero-cost until write).
    BtrfsReflink,
    /// Windows ReFS CoW via `FSCTL_DUPLICATE_EXTENTS_TO_FILE` (zero-cost until write).
    RefsClone,
}

impl CopyStrategy {
    /// Human-readable description for logging and user-facing output.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Full => "full copy",
            Self::ApfsClone => "APFS clone (COW)",
            Self::BtrfsReflink => "Btrfs reflink (COW)",
            Self::RefsClone => "ReFS clone (COW)",
        }
    }

    /// Returns true if this strategy uses copy-on-write (zero I/O until first write).
    pub fn is_cow(&self) -> bool {
        matches!(self, Self::ApfsClone | Self::BtrfsReflink | Self::RefsClone)
    }
}

/// Statistics from workspace creation — used for benchmarking and diagnostics.
#[derive(Debug, Clone)]
pub struct CopyStat {
    /// Strategy that was used for this workspace.
    pub strategy: CopyStrategy,
    /// Wall-clock time for the entire staging copy.
    pub duration: Duration,
    /// Number of files copied (or cloned).
    pub files_copied: usize,
    /// Sum of source file sizes in bytes (COW copies report the same number
    /// but consume no extra disk space until the agent modifies the files).
    pub bytes_total: u64,
    /// Number of directories symlinked (smart mode only). Zero for full/refs-cow.
    pub symlinks_created: usize,
    /// Estimated bytes behind symlinks (for the staging size report message).
    pub bytes_symlinked: u64,
}

impl CopyStat {
    pub(crate) fn new(strategy: CopyStrategy) -> Self {
        Self {
            strategy,
            duration: Duration::ZERO,
            files_copied: 0,
            bytes_total: 0,
            symlinks_created: 0,
            bytes_symlinked: 0,
        }
    }

    /// Format a human-readable staging size report.
    pub fn size_report(&self) -> String {
        if self.symlinks_created > 0 {
            let copied_mb = self.bytes_total as f64 / (1024.0 * 1024.0);
            let symlinked_gb = self.bytes_symlinked as f64 / (1024.0 * 1024.0 * 1024.0);
            let reduction = if self.bytes_total > 0 && self.bytes_symlinked > 0 {
                let ratio =
                    (self.bytes_total + self.bytes_symlinked) as f64 / self.bytes_total as f64;
                format!("  ({:.0}× reduction)", ratio)
            } else {
                String::new()
            };
            format!(
                "Staging: {:.1} MB copied, {:.1} GB symlinked (smart mode){} in {:.1}s",
                copied_mb,
                symlinked_gb,
                reduction,
                self.duration.as_secs_f64()
            )
        } else if self.strategy.is_cow() {
            format!(
                "Staging: {} files ({:.1} MB) via {} in {:.1}s",
                self.files_copied,
                self.bytes_total as f64 / (1024.0 * 1024.0),
                self.strategy.description(),
                self.duration.as_secs_f64()
            )
        } else {
            format!(
                "Staging: {} files ({:.1} MB) copied in {:.1}s",
                self.files_copied,
                self.bytes_total as f64 / (1024.0 * 1024.0),
                self.duration.as_secs_f64()
            )
        }
    }
}

/// Probe the staging directory to determine the best available copy strategy.
///
/// Creates a tiny temp file inside `staging_dir`, attempts to clone it using
/// the platform-native COW method, then deletes both files. Returns
/// [`CopyStrategy::Full`] if no COW support is detected.
///
/// Called once per workspace creation — negligible overhead.
pub fn detect_strategy(_staging_dir: &Path) -> CopyStrategy {
    #[cfg(windows)]
    if probe_refs_clone(_staging_dir) {
        return CopyStrategy::RefsClone;
    }

    #[cfg(target_os = "macos")]
    if probe_apfs_clone(_staging_dir) {
        return CopyStrategy::ApfsClone;
    }

    #[cfg(target_os = "linux")]
    if probe_btrfs_reflink(_staging_dir) {
        return CopyStrategy::BtrfsReflink;
    }

    CopyStrategy::Full
}

/// Copy a single file using the specified strategy.
///
/// Returns the number of bytes of actual I/O performed (`0` for COW copies
/// until the file is modified). Falls back to `fs::copy` if the COW method
/// fails for any reason (e.g., cross-device copy, partial APFS support).
pub fn copy_file_with_strategy(src: &Path, dst: &Path, strategy: CopyStrategy) -> io::Result<u64> {
    match strategy {
        CopyStrategy::Full => std::fs::copy(src, dst),

        #[cfg(windows)]
        CopyStrategy::RefsClone => {
            if windows::clone_file(src, dst) {
                Ok(0) // COW: no data copied
            } else {
                std::fs::copy(src, dst)
            }
        }

        #[cfg(target_os = "macos")]
        CopyStrategy::ApfsClone => {
            if macos::clone_file(src, dst) {
                Ok(0) // COW: no data copied
            } else {
                std::fs::copy(src, dst)
            }
        }

        #[cfg(target_os = "linux")]
        CopyStrategy::BtrfsReflink => {
            match linux::clone_file(src, dst) {
                Ok(true) => Ok(0), // COW: no data copied
                _ => std::fs::copy(src, dst),
            }
        }

        // Safety fallback for unexpected platform/strategy combinations.
        #[allow(unreachable_patterns)]
        _ => std::fs::copy(src, dst),
    }
}

// ── Windows ReFS clone (FSCTL_DUPLICATE_EXTENTS_TO_FILE) ────────

#[cfg(windows)]
mod windows {
    use std::path::Path;

    /// Clone a file using `FSCTL_DUPLICATE_EXTENTS_TO_FILE` (ReFS CoW).
    /// Returns true on success; callers fall back to `std::fs::copy` on failure.
    pub fn clone_file(src: &Path, dst: &Path) -> bool {
        // Delegate to overlay.rs windows module implementation.
        // Duplicate the logic here for use in the file-copy path.
        use std::os::windows::io::AsRawHandle;

        let src_file = match std::fs::File::open(src) {
            Ok(f) => f,
            Err(_) => return false,
        };
        let dst_file = match std::fs::File::create(dst) {
            Ok(f) => f,
            Err(_) => return false,
        };

        let src_size = match src_file.metadata() {
            Ok(m) => m.len(),
            Err(_) => return false,
        };

        if src_size == 0 {
            return true; // Empty file already created above.
        }

        #[repr(C)]
        struct DuplicateExtentsData {
            file_handle: isize,
            source_file_offset: i64,
            target_file_offset: i64,
            byte_count: i64,
        }

        // Pre-allocate the destination file.
        let set_ok = unsafe {
            windows_sys::Win32::Storage::FileSystem::SetEndOfFile(dst_file.as_raw_handle() as _)
        };
        if set_ok == 0 {
            return false;
        }

        let params = DuplicateExtentsData {
            file_handle: src_file.as_raw_handle() as isize,
            source_file_offset: 0,
            target_file_offset: 0,
            byte_count: src_size as i64,
        };

        const FSCTL_DUPLICATE_EXTENTS_TO_FILE: u32 = 0x0009_8344;
        let mut bytes_returned: u32 = 0;

        // SAFETY: standard ReFS CoW API with correctly sized parameters.
        let ok = unsafe {
            windows_sys::Win32::System::IO::DeviceIoControl(
                dst_file.as_raw_handle() as _,
                FSCTL_DUPLICATE_EXTENTS_TO_FILE,
                &params as *const _ as *const _,
                std::mem::size_of::<DuplicateExtentsData>() as u32,
                std::ptr::null_mut(),
                0,
                &mut bytes_returned,
                std::ptr::null_mut(),
            )
        };

        ok != 0
    }
}

/// Probe whether ReFS clone works in `dir`.
#[cfg(windows)]
fn probe_refs_clone(dir: &Path) -> bool {
    let pid = std::process::id();
    let src = dir.join(format!(".ta-probe-{}-src", pid));
    let dst = dir.join(format!(".ta-probe-{}-dst", pid));

    if std::fs::write(&src, b"ta-cow-probe").is_err() {
        return false;
    }

    let result = windows::clone_file(&src, &dst);

    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&dst);

    result
}

// ── macOS APFS clone (clonefile(2)) ────────────────────────────

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int, c_uint};
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    extern "C" {
        // clonefile(2): available macOS 10.12+ in libSystem.B.dylib.
        // Always linked on macOS — no extra crate dependency required.
        // Creates a copy-on-write clone: zero data I/O, zero disk space
        // consumed until the clone or original is modified.
        fn clonefile(src: *const c_char, dst: *const c_char, flags: c_uint) -> c_int;
    }

    /// Clone a file using clonefile(2). Returns true on success.
    pub fn clone_file(src: &Path, dst: &Path) -> bool {
        let src_bytes = src.as_os_str().as_bytes();
        let dst_bytes = dst.as_os_str().as_bytes();

        let src_cstr = match CString::new(src_bytes) {
            Ok(s) => s,
            Err(_) => return false,
        };
        let dst_cstr = match CString::new(dst_bytes) {
            Ok(s) => s,
            Err(_) => return false,
        };

        // SAFETY: pointers are valid C strings from CString; clonefile is
        // a standard macOS syscall in libSystem.B.dylib.
        let ret = unsafe { clonefile(src_cstr.as_ptr(), dst_cstr.as_ptr(), 0) };
        ret == 0
    }
}

/// Probe whether APFS clone works in `dir` by cloning a temp file.
#[cfg(target_os = "macos")]
fn probe_apfs_clone(dir: &Path) -> bool {
    let pid = std::process::id();
    let src = dir.join(format!(".ta-probe-{}-src", pid));
    let dst = dir.join(format!(".ta-probe-{}-dst", pid));

    if std::fs::write(&src, b"ta-cow-probe").is_err() {
        return false;
    }

    let result = macos::clone_file(&src, &dst);

    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&dst);

    result
}

// ── Linux Btrfs reflink (FICLONE ioctl) ────────────────────────

#[cfg(target_os = "linux")]
mod linux {
    use std::io;
    use std::os::unix::io::AsRawFd;
    use std::path::Path;

    // FICLONE ioctl: _IOW(0x94, 9, int) = 0x40049409
    // Supported on Btrfs, XFS (Linux 4.5+), and OCFS2.
    const FICLONE: libc::c_ulong = 0x40049409;

    /// Clone a file using the FICLONE ioctl. Returns Ok(true) on success.
    pub fn clone_file(src: &Path, dst: &Path) -> io::Result<bool> {
        let src_file = std::fs::File::open(src)?;
        let dst_file = std::fs::File::create(dst)?;

        // SAFETY: FICLONE takes a single int (src fd) as its third argument.
        // Both file descriptors are valid for the duration of this call.
        // FICLONE is u64 on some targets (e.g. aarch64-linux-musl) but libc::ioctl
        // expects libc::Ioctl (i32 on musl, c_ulong on glibc). Cast via as to
        // satisfy both without panicking — ioctl request codes fit in 32 bits.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let ret = unsafe {
            libc::ioctl(
                dst_file.as_raw_fd(),
                FICLONE as libc::Ioctl,
                src_file.as_raw_fd(),
            )
        };

        Ok(ret == 0)
    }
}

/// Probe whether Btrfs reflink works in `dir` by cloning a temp file.
#[cfg(target_os = "linux")]
fn probe_btrfs_reflink(dir: &Path) -> bool {
    let pid = std::process::id();
    let src = dir.join(format!(".ta-probe-{}-src", pid));
    let dst = dir.join(format!(".ta-probe-{}-dst", pid));

    if std::fs::write(&src, b"ta-cow-probe").is_err() {
        return false;
    }

    let result = linux::clone_file(&src, &dst).unwrap_or(false);

    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&dst);

    result
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn copy_strategy_description() {
        assert_eq!(CopyStrategy::Full.description(), "full copy");
        assert!(!CopyStrategy::Full.is_cow());

        #[cfg(target_os = "macos")]
        {
            assert!(CopyStrategy::ApfsClone.description().contains("APFS"));
            assert!(CopyStrategy::ApfsClone.is_cow());
        }

        #[cfg(target_os = "linux")]
        {
            assert!(CopyStrategy::BtrfsReflink.description().contains("Btrfs"));
            assert!(CopyStrategy::BtrfsReflink.is_cow());
        }

        #[cfg(windows)]
        {
            assert!(CopyStrategy::RefsClone.description().contains("ReFS"));
            assert!(CopyStrategy::RefsClone.is_cow());
        }
    }

    #[test]
    fn refs_clone_is_cow() {
        // RefsClone must always report as COW regardless of platform.
        assert!(CopyStrategy::RefsClone.is_cow());
        assert!(CopyStrategy::RefsClone.description().contains("ReFS"));
    }

    #[test]
    fn detect_strategy_returns_a_strategy() {
        // detect_strategy should always return something — even if it's Full.
        let dir = TempDir::new().unwrap();
        let strategy = detect_strategy(dir.path());
        // Just verify it doesn't panic and returns a valid variant.
        let _ = strategy.description();
    }

    #[test]
    fn copy_file_full_strategy_copies_content() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src.txt");
        let dst = dir.path().join("dst.txt");

        std::fs::write(&src, b"hello world").unwrap();
        let bytes = copy_file_with_strategy(&src, &dst, CopyStrategy::Full).unwrap();

        assert!(bytes > 0, "full copy should report non-zero bytes");
        assert_eq!(std::fs::read(&dst).unwrap(), b"hello world");
    }

    #[test]
    fn copy_stat_accumulates() {
        let mut stat = CopyStat::new(CopyStrategy::Full);
        assert_eq!(stat.files_copied, 0);
        assert_eq!(stat.bytes_total, 0);

        stat.files_copied += 1;
        stat.bytes_total += 42;

        assert_eq!(stat.files_copied, 1);
        assert_eq!(stat.bytes_total, 42);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn apfs_clone_probe_and_copy() {
        let dir = TempDir::new().unwrap();

        // Probe should succeed on APFS (standard macOS dev filesystem).
        // It may return false on CI with non-APFS tmpfs — that's OK.
        let strategy = detect_strategy(dir.path());

        if strategy == CopyStrategy::ApfsClone {
            // If probe succeeded, actual file copy should also work.
            let src = dir.path().join("source.txt");
            let dst = dir.path().join("clone.txt");
            std::fs::write(&src, b"test content for clone").unwrap();

            let bytes = copy_file_with_strategy(&src, &dst, CopyStrategy::ApfsClone).unwrap();
            assert_eq!(bytes, 0, "COW clone should report 0 I/O bytes");
            assert_eq!(
                std::fs::read(&dst).unwrap(),
                b"test content for clone",
                "cloned content must match source"
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn btrfs_reflink_probe_and_copy() {
        let dir = TempDir::new().unwrap();

        // On non-Btrfs (common in CI), probe returns Full — that's expected.
        let strategy = detect_strategy(dir.path());

        if strategy == CopyStrategy::BtrfsReflink {
            let src = dir.path().join("source.txt");
            let dst = dir.path().join("reflink.txt");
            std::fs::write(&src, b"test content for reflink").unwrap();

            let bytes = copy_file_with_strategy(&src, &dst, CopyStrategy::BtrfsReflink).unwrap();
            assert_eq!(bytes, 0, "COW reflink should report 0 I/O bytes");
            assert_eq!(std::fs::read(&dst).unwrap(), b"test content for reflink");
        }
    }
}
