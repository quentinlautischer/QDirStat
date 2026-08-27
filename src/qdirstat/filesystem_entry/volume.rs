//! How large a volume is and how much of it is in use.
//!
//! There is no disk-space API in std, so this is a direct call to the platform. Only
//! Windows is implemented: its one documented entry point takes a path and fills in
//! plain 64-bit counters. The unix equivalent, statvfs, has a struct whose field order
//! and word size differ between Linux, macOS and 32/64-bit builds, and declaring it by
//! hand would read the wrong offsets rather than fail loudly, so unix reports None and
//! the scan falls back to a spinner without a percentage.

/// What the filesystem says about a volume, independent of anything QDirStat scanned.
pub struct VolumeUsage {
    /// Capacity of the volume.
    pub total: u64,
    /// Bytes in use, as the filesystem accounts for them.
    pub used: u64,
}

impl VolumeUsage {
    /// How full the volume is. This is the filesystem's own figure and has nothing to
    /// do with how much of it a scan managed to walk.
    pub fn percent_full(&self) -> u8 {
        if self.total == 0 {
            return 0;
        }

        // Rounded, unlike scan coverage: this is a fixed reading rather than a progress
        // figure, so nearest is more honest than "at least". u128 keeps the multiply safe.
        let used = self.used as u128;
        let total = self.total as u128;

        ((used * 100 + total / 2) / total).min(100) as u8
    }
}

/// Capacity and usage of the volume holding `path`, or None when the platform cannot say.
#[cfg(windows)]
pub fn usage(path: &str) -> Option<VolumeUsage> {
    // Wide, NUL-terminated: the W entry point reads until the terminator.
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();

    let mut available_to_caller: u64 = 0;
    let mut total: u64 = 0;
    let mut free: u64 = 0;

    // SAFETY: `wide` is NUL-terminated and outlives the call, and the three out-params
    // are live u64s for the duration. The counters are ULARGE_INTEGER, which is a union
    // over a 64-bit unsigned value, so u64 matches its layout.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available_to_caller,
            &mut total,
            &mut free,
        )
    };

    if ok == 0 || total == 0 {
        return None;
    }

    Some(VolumeUsage {
        total,
        used: total.saturating_sub(free),
    })
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetDiskFreeSpaceExW(
        directory_name: *const u16,
        free_bytes_available_to_caller: *mut u64,
        total_number_of_bytes: *mut u64,
        total_number_of_free_bytes: *mut u64,
    ) -> i32;
}

#[cfg(not(windows))]
pub fn usage(_path: &str) -> Option<VolumeUsage> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fullness_is_used_over_capacity() {
        let half = VolumeUsage { total: 1000, used: 500 };
        assert_eq!(half.percent_full(), 50);

        let empty = VolumeUsage { total: 1000, used: 0 };
        assert_eq!(empty.percent_full(), 0);

        let full = VolumeUsage { total: 1000, used: 1000 };
        assert_eq!(full.percent_full(), 100);
    }

    #[test]
    fn fullness_rounds_to_nearest() {
        // 95.8% full reads as 96, matching what the OS reports, not 95.
        let nearly_full = VolumeUsage { total: 1000, used: 958 };
        assert_eq!(nearly_full.percent_full(), 96);

        let just_under = VolumeUsage { total: 1000, used: 954 };
        assert_eq!(just_under.percent_full(), 95);
    }

    #[test]
    fn fullness_survives_a_nonsense_volume() {
        let nothing = VolumeUsage { total: 0, used: 0 };
        assert_eq!(nothing.percent_full(), 0, "no dividing by zero");
    }

    #[cfg(windows)]
    #[test]
    fn reports_plausible_usage_for_a_real_volume() {
        let usage = usage("C:\\").expect("C:\\ should report disk usage");

        assert!(usage.total > 0, "a volume has a size");
        assert!(usage.used > 0, "a system volume is never empty");
        assert!(usage.used <= usage.total, "cannot use more than exists");
        // 1 PB: a sanity bound that catches a garbled call without pinning a real size.
        assert!(usage.total < 1024 * 1024 * 1024 * 1024 * 1024, "implausible size");
    }

    #[cfg(windows)]
    #[test]
    fn returns_none_for_a_volume_that_does_not_exist() {
        assert!(usage("\\\\?\\Volume{00000000-0000-0000-0000-000000000000}\\").is_none());
    }

    #[cfg(not(windows))]
    #[test]
    fn reports_nothing_off_windows() {
        assert!(usage("/").is_none());
    }
}
