//! This PC's time for a Device's clock: seconds since 1970 in UTC and the local offset.
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn unix_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Minutes east of UTC, daylight saving included, within the protocol's ±840.
#[cfg(windows)]
pub(crate) fn offset_min() -> i32 {
    use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    const DAYLIGHT: u32 = 2;
    const STANDARD: u32 = 1;
    // SAFETY: the call only writes the zeroed struct it is given.
    let (zone, info) = unsafe {
        let mut info: TIME_ZONE_INFORMATION = std::mem::zeroed();
        (GetTimeZoneInformation(&mut info), info)
    };
    let bias = match zone {
        DAYLIGHT => info.Bias + info.DaylightBias,
        STANDARD => info.Bias + info.StandardBias,
        _ => info.Bias,
    };
    (-bias).clamp(-840, 840)
}

#[cfg(unix)]
pub(crate) fn offset_min() -> i32 {
    let now = unix_s() as libc::time_t;
    // SAFETY: localtime_r only writes the zeroed struct it is given.
    let gmtoff = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            0
        } else {
            tm.tm_gmtoff
        }
    };
    ((gmtoff / 60) as i32).clamp(-840, 840)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_time_and_offset_are_in_the_protocol_range() {
        assert!(super::unix_s() > 1_700_000_000);
        assert!(super::unix_s() <= td_link::photo::CLOCK_UNIX_S_MAX);
        assert!(td_link::photo::CLOCK_OFFSET_MIN.contains(&super::offset_min()));
    }
}
