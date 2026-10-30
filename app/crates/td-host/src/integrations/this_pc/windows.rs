use super::{CounterSample, CounterSource};
use std::{collections::BTreeMap, ptr};
use windows_sys::Win32::{
    NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2},
    System::{
        Performance::*,
        SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
    },
};

pub struct Counters {
    query: PDH_HQUERY,
    cpu: PDH_HCOUNTER,
    disk: PDH_HCOUNTER,
    read: PDH_HCOUNTER,
    write: PDH_HCOUNTER,
    gpu: PDH_HCOUNTER,
    network: Vec<(PDH_HCOUNTER, PDH_HCOUNTER)>,
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
            network: Vec::new(),
        };
        if unsafe { PdhOpenQueryW(ptr::null(), 0, &mut this.query) } != 0 {
            return this;
        }
        this.cpu = this.add(r"\Processor Information(_Total)\% Processor Utility");
        this.disk = this.add(r"\PhysicalDisk(_Total)\% Idle Time");
        this.read = this.add(r"\PhysicalDisk(_Total)\Disk Read Bytes/sec");
        this.write = this.add(r"\PhysicalDisk(_Total)\Disk Write Bytes/sec");
        this.gpu = this.add(r"\GPU Engine(*)\Utilization Percentage");
        for name in physical_adapters() {
            this.network.push((
                this.add(&format!(r"\Network Interface({name})\Bytes Received/sec")),
                this.add(&format!(r"\Network Interface({name})\Bytes Sent/sec")),
            ));
        }
        this
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
fn physical_adapters() -> Vec<String> {
    let mut table = ptr::null_mut();
    if unsafe { GetIfTable2(&mut table) } != 0 {
        return Vec::new();
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
    names
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
            let used = memory.ullTotalPhys - memory.ullAvailPhys;
            sample.values.insert(
                "ram_pct".into(),
                used as f64 * 100.0 / memory.ullTotalPhys as f64,
            );
            sample.values.insert("ram_used_bytes".into(), used as f64);
            sample
                .values
                .insert("ram_total_bytes".into(), memory.ullTotalPhys as f64);
        }
        if self.query.is_null() || unsafe { PdhCollectQueryData(self.query) } != 0 {
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
        if !self.network.is_empty() {
            for (id, receive) in [
                ("network_receive_bytes_s", true),
                ("network_send_bytes_s", false),
            ] {
                let sum: Option<u64> = self
                    .network
                    .iter()
                    .map(|(rx, tx)| raw(if receive { *rx } else { *tx }))
                    .try_fold(0u64, |sum, value| sum.checked_add(value?));
                if let Some(sum) = sum {
                    sample.totals.insert(id.into(), sum);
                }
            }
        }
        sample
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
