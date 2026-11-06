use super::{CounterSample, CounterSource};
use std::{
    collections::{BTreeMap, BTreeSet},
    ptr,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2},
    System::{
        Performance::*,
        SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
    },
};

/// How often `collect` looks for network adapters that appeared or went away. One adapter-table
/// read every ten one-second samples costs next to nothing, and a newly plugged adapter starts
/// counting within about ten seconds.
const ADAPTER_RESCAN: Duration = Duration::from_secs(10);

pub struct Counters {
    query: PDH_HQUERY,
    cpu: PDH_HCOUNTER,
    disk: PDH_HCOUNTER,
    read: PDH_HCOUNTER,
    write: PDH_HCOUNTER,
    gpu: PDH_HCOUNTER,
    network: BTreeMap<String, Adapter>,
    scanned: Instant,
    /// Running receive and send byte totals across every adapter counted so far.
    traffic: [u64; 2],
}
/// One physical adapter's receive and send counters and their last raw byte counts.
struct Adapter {
    counters: [PDH_HCOUNTER; 2],
    last: [Option<u64>; 2],
}
// PDH handles have no thread affinity; the Integration owns them exclusively.
unsafe impl Send for Counters {}
impl Counters {
    pub fn new() -> Self {
        let mut this = Self {
            query: ptr::null_mut(),
            cpu: ptr::null_mut(),
            disk: ptr::null_mut(),
            read: ptr::null_mut(),
            write: ptr::null_mut(),
            gpu: ptr::null_mut(),
            network: BTreeMap::new(),
            scanned: Instant::now(),
            traffic: [0; 2],
        };
        if unsafe { PdhOpenQueryW(ptr::null(), 0, &mut this.query) } != 0 {
            return this;
        }
        this.cpu = this.add(r"\Processor Information(_Total)\% Processor Utility");
        this.disk = this.add(r"\PhysicalDisk(_Total)\% Idle Time");
        this.read = this.add(r"\PhysicalDisk(_Total)\Disk Read Bytes/sec");
        this.write = this.add(r"\PhysicalDisk(_Total)\Disk Write Bytes/sec");
        this.gpu = this.add(r"\GPU Engine(*)\Utilization Percentage");
        this.rescan_adapters();
        this
    }
    fn rescan_adapters(&mut self) {
        self.scanned = Instant::now();
        // A failed adapter-table read keeps the current counters rather than dropping them all.
        if let Some(present) = physical_adapters() {
            self.count_adapters(&present);
        }
    }
    /// Starts counting adapters in `present` that aren't counted yet and stops counting the ones
    /// missing from it. The other counters in the query keep their history, so CPU, disk and GPU
    /// values are not disturbed.
    fn count_adapters(&mut self, present: &BTreeSet<String>) {
        let counted: BTreeSet<String> = self.network.keys().cloned().collect();
        let (added, removed) = adapter_changes(&counted, present);
        for name in removed {
            let Some(adapter) = self.network.remove(name) else {
                continue;
            };
            for counter in adapter.counters {
                if !counter.is_null() {
                    // The handle came from PdhAddEnglishCounterW on this query and left the map
                    // above, so nothing uses it after removal.
                    unsafe {
                        PdhRemoveCounter(counter);
                    }
                }
            }
        }
        for name in added {
            let counters = [
                self.add(&format!(r"\Network Interface({name})\Bytes Received/sec")),
                self.add(&format!(r"\Network Interface({name})\Bytes Sent/sec")),
            ];
            // An adapter missing either counter stays out of the map, so the next scan retries
            // both. The counter that was added leaves the query first.
            if counters.iter().any(|counter| counter.is_null()) {
                for counter in counters {
                    if !counter.is_null() {
                        // The handle was just added to this query and is never stored.
                        unsafe {
                            PdhRemoveCounter(counter);
                        }
                    }
                }
                continue;
            }
            self.network.insert(
                name.clone(),
                Adapter {
                    counters,
                    last: [None; 2],
                },
            );
        }
    }
    fn add(&self, path: &str) -> PDH_HCOUNTER {
        let path: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        let mut counter = ptr::null_mut();
        if unsafe { PdhAddEnglishCounterW(self.query, path.as_ptr(), 0, &mut counter) } == 0 {
            counter
        } else {
            ptr::null_mut()
        }
    }
}
impl Drop for Counters {
    fn drop(&mut self) {
        if !self.query.is_null() {
            unsafe {
                PdhCloseQuery(self.query);
            }
        }
    }
}
/// Which adapter names to start counting and which to stop counting.
fn adapter_changes<'a>(
    counted: &'a BTreeSet<String>,
    present: &'a BTreeSet<String>,
) -> (Vec<&'a String>, Vec<&'a String>) {
    (
        present.difference(counted).collect(),
        counted.difference(present).collect(),
    )
}
/// Bytes an adapter moved since its last raw reading, recording `now` as the new baseline. A
/// first reading, one after a missed reading and a counter that went backwards only set the
/// baseline, so bytes an adapter moved before it was counted never show up as one huge second.
fn bytes_since(last: &mut Option<u64>, now: Option<u64>) -> u64 {
    let moved = match (*last, now) {
        (Some(before), Some(now)) => now.saturating_sub(before),
        _ => 0,
    };
    *last = now;
    moved
}
/// PDH instance names of the physical adapters, or `None` when the adapter table can't be read.
fn physical_adapters() -> Option<BTreeSet<String>> {
    let mut table = ptr::null_mut();
    if unsafe { GetIfTable2(&mut table) } != 0 || table.is_null() {
        return None;
    }
    let rows = unsafe {
        std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize)
    };
    let names = rows
        .iter()
        .filter(|row| row.InterfaceAndOperStatusFlags._bitfield & 1 != 0)
        .map(|row| {
            let end = row
                .Description
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(row.Description.len());
            String::from_utf16_lossy(&row.Description[..end])
                .replace('(', "[")
                .replace(')', "]")
                .replace(['#', '/', '\\'], "_")
        })
        .collect();
    unsafe {
        FreeMibTable(table.cast());
    }
    Some(names)
}
fn formatted(counter: PDH_HCOUNTER) -> Option<f64> {
    if counter.is_null() {
        return None;
    }
    let mut value = PDH_FMT_COUNTERVALUE::default();
    if unsafe { PdhGetFormattedCounterValue(counter, PDH_FMT_DOUBLE, ptr::null_mut(), &mut value) }
        != 0
        || value.CStatus > 1
    {
        return None;
    }
    Some(unsafe { value.Anonymous.doubleValue })
}
fn raw(counter: PDH_HCOUNTER) -> Option<u64> {
    if counter.is_null() {
        return None;
    }
    let mut value = PDH_RAW_COUNTER::default();
    if unsafe { PdhGetRawCounterValue(counter, ptr::null_mut(), &mut value) } != 0
        || value.CStatus > 1
    {
        return None;
    }
    u64::try_from(value.FirstValue).ok()
}
fn gpu(counter: PDH_HCOUNTER) -> Option<f64> {
    if counter.is_null() {
        return None;
    }
    let (mut bytes, mut count) = (0, 0);
    if unsafe {
        PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_DOUBLE,
            &mut bytes,
            &mut count,
            ptr::null_mut(),
        )
    } != PDH_MORE_DATA
    {
        return None;
    }
    let mut buffer = vec![0u64; (bytes as usize).div_ceil(8)];
    let items = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
    if unsafe {
        PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut bytes, &mut count, items)
    } != 0
    {
        return None;
    }
    let mut engines: BTreeMap<String, f64> = BTreeMap::new();
    for item in unsafe { std::slice::from_raw_parts(items, count as usize) } {
        if item.FmtValue.CStatus > 1 {
            continue;
        }
        let mut len = 0;
        while unsafe { *item.szName.add(len) } != 0 {
            len += 1;
        }
        let name =
            String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(item.szName, len) });
        if let Some((_, kind)) = name.rsplit_once("engtype_") {
            *engines.entry(kind.into()).or_default() +=
                unsafe { item.FmtValue.Anonymous.doubleValue };
        }
    }
    engines.values().copied().reduce(f64::max)
}
impl CounterSource for Counters {
    fn collect(&mut self) -> CounterSample {
        let mut sample = CounterSample::default();
        let mut memory = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        if unsafe { GlobalMemoryStatusEx(&mut memory) } != 0 && memory.ullTotalPhys > 0 {
            let used = memory.ullTotalPhys.saturating_sub(memory.ullAvailPhys);
            sample.values.insert(
                "ram_pct".into(),
                used as f64 * 100.0 / memory.ullTotalPhys as f64,
            );
            sample.values.insert("ram_used_bytes".into(), used as f64);
            sample
                .values
                .insert("ram_total_bytes".into(), memory.ullTotalPhys as f64);
        }
        if self.query.is_null() {
            return sample;
        }
        if self.scanned.elapsed() >= ADAPTER_RESCAN {
            self.rescan_adapters();
        }
        if unsafe { PdhCollectQueryData(self.query) } != 0 {
            return sample;
        }
        for (id, value) in [
            ("cpu_pct", formatted(self.cpu)),
            (
                "disk_active_pct",
                formatted(self.disk).map(|idle| 100.0 - idle),
            ),
            ("gpu_pct", gpu(self.gpu)),
        ] {
            if let Some(value) = value {
                sample.values.insert(id.into(), value.max(0.0));
            }
        }
        for (id, value) in [
            ("disk_read_bytes_s", raw(self.read)),
            ("disk_write_bytes_s", raw(self.write)),
        ] {
            if let Some(value) = value {
                sample.totals.insert(id.into(), value);
            }
        }
        // Adapters come and go, so the Integration gets running totals built from each adapter's
        // own deltas rather than a sum of raw counts that would jump when an adapter joins.
        for (direction, id) in ["network_receive_bytes_s", "network_send_bytes_s"]
            .into_iter()
            .enumerate()
        {
            let mut read_any = false;
            for adapter in self.network.values_mut() {
                let now = raw(adapter.counters[direction]);
                read_any |= now.is_some();
                self.traffic[direction] = self.traffic[direction]
                    .wrapping_add(bytes_since(&mut adapter.last[direction], now));
            }
            if read_any {
                sample.totals.insert(id.into(), self.traffic[direction]);
            }
        }
        sample
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn names(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|name| (*name).to_owned()).collect()
    }
    #[test]
    fn adapter_changes_add_new_and_remove_gone_adapters() {
        let counted = names(&["Ethernet", "Wi-Fi"]);
        let present = names(&["Ethernet", "USB Ethernet"]);
        let (added, removed) = adapter_changes(&counted, &present);
        assert_eq!(added, ["USB Ethernet"]);
        assert_eq!(removed, ["Wi-Fi"]);
        let (added, removed) = adapter_changes(&present, &present);
        assert!(added.is_empty() && removed.is_empty());
    }
    #[test]
    fn late_adapter_adds_only_bytes_moved_after_it_was_counted() {
        let (mut ethernet, mut dongle) = (None, None);
        let mut total = bytes_since(&mut ethernet, Some(1_000));
        assert_eq!(total, 0);
        total += bytes_since(&mut ethernet, Some(1_500));
        // The dongle already moved 9 GB before it was plugged into this PC.
        total += bytes_since(&mut dongle, Some(9_000_000_000));
        assert_eq!(total, 500);
        total += bytes_since(&mut ethernet, Some(1_600));
        total += bytes_since(&mut dongle, Some(9_000_000_250));
        assert_eq!(total, 850);
    }
    #[test]
    fn missed_or_reset_reading_only_sets_a_new_baseline() {
        let mut adapter = Some(5_000);
        assert_eq!(bytes_since(&mut adapter, None), 0);
        assert_eq!(bytes_since(&mut adapter, Some(8_000)), 0);
        assert_eq!(bytes_since(&mut adapter, Some(100)), 0);
        assert_eq!(bytes_since(&mut adapter, Some(400)), 300);
    }
    #[test]
    fn windows_pdh_smoke() {
        let mut counters = Counters::new();
        assert!(!counters.query.is_null());
        counters.collect();
        std::thread::sleep(std::time::Duration::from_secs(1));
        let sample = counters.collect();
        assert!(sample.values.contains_key("cpu_pct"));
        assert!(sample.values.contains_key("ram_pct"));
        for id in ["gpu_pct", "disk_active_pct"] {
            eprintln!(
                "{id}: {}",
                if sample.values.contains_key(id) {
                    "available"
                } else {
                    "unavailable"
                }
            );
        }
        for id in [
            "disk_read_bytes_s",
            "disk_write_bytes_s",
            "network_receive_bytes_s",
            "network_send_bytes_s",
        ] {
            eprintln!(
                "{id}: {}",
                if sample.totals.contains_key(id) {
                    "available"
                } else {
                    "unavailable"
                }
            );
        }
    }
    #[test]
    fn windows_pdh_adapter_that_appears_later_counts_without_a_spike() {
        let mut counters = Counters::new();
        let present: BTreeSet<String> = counters.network.keys().cloned().collect();
        // PDH can accept a counter for an instance it then can't collect, so skip unless an
        // adapter really reads.
        let sample = counters.collect();
        if ["network_receive_bytes_s", "network_send_bytes_s"]
            .iter()
            .any(|id| !sample.totals.contains_key(*id))
        {
            eprintln!("no physical adapter PDH can read; skipped");
            return;
        }
        // Every adapter goes away: its counters leave the query and network has no reading.
        counters.count_adapters(&BTreeSet::new());
        assert!(counters.network.is_empty());
        let sample = counters.collect();
        assert!(!sample.totals.contains_key("network_receive_bytes_s"));
        // The same adapters come back as if just plugged in. Their bytes since boot stay out.
        let before = counters.traffic;
        counters.count_adapters(&present);
        assert_eq!(counters.network.len(), present.len());
        let sample = counters.collect();
        assert_eq!(
            sample.totals.get("network_receive_bytes_s"),
            Some(&before[0])
        );
        assert_eq!(sample.totals.get("network_send_bytes_s"), Some(&before[1]));
        std::thread::sleep(Duration::from_secs(1));
        let sample = counters.collect();
        assert!(sample.values.contains_key("cpu_pct"));
        for (direction, id) in ["network_receive_bytes_s", "network_send_bytes_s"]
            .into_iter()
            .enumerate()
        {
            let total = sample.totals.get(id).copied();
            assert!(
                total.is_some(),
                "{id} has no reading after the adapters returned"
            );
            eprintln!(
                "{id}: {} bytes in about 1 s",
                total.unwrap_or_default() - before[direction]
            );
        }
    }
}
