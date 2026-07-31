use crate::mac::sysctl::{sysctl_u32, sysctl_u64};
use crate::sampler::{MemDetail, PressureLevel};

// Field layout verified against
// $(xcrun --show-sdk-path)/usr/include/mach/vm_statistics.h (rev1 of
// struct vm_statistics64). The kernel's host_statistics64() supports older
// "rev" struct sizes via the `count` in/out parameter, so requesting only
// the rev1-sized struct here (as opposed to the full modern struct with
// rev2/rev3 fields like `swapped_count`) is intentional and safe.
#[repr(C)]
#[derive(Default)]
struct VmStatistics64 {
    free_count: u32,
    active_count: u32,
    inactive_count: u32,
    wire_count: u32,
    zero_fill_count: u64,
    reactivations: u64,
    pageins: u64,
    pageouts: u64,
    faults: u64,
    cow_faults: u64,
    lookups: u64,
    hits: u64,
    purges: u64,
    purgeable_count: u32,
    speculative_count: u32,
    decompressions: u64,
    compressions: u64,
    swapins: u64,
    swapouts: u64,
    compressor_page_count: u32,
    throttled_count: u32,
    external_page_count: u32,
    internal_page_count: u32,
    total_uncompressed_pages_in_compressor: u64,
}

const HOST_VM_INFO64: i32 = 4;

extern "C" {
    fn mach_host_self() -> u32;
    fn host_statistics64(host: u32, flavor: i32, info: *mut i32, count: *mut u32) -> i32;
}

fn vm_stats() -> Option<VmStatistics64> {
    let mut stats = VmStatistics64::default();
    let mut count = (std::mem::size_of::<VmStatistics64>() / 4) as u32;
    let rc = unsafe {
        host_statistics64(
            mach_host_self(),
            HOST_VM_INFO64,
            &mut stats as *mut _ as *mut i32,
            &mut count,
        )
    };
    (rc == 0).then_some(stats)
}

fn swap() -> (u64, u64) {
    let mut xsw: libc::xsw_usage = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::xsw_usage>();
    let name = std::ffi::CString::new("vm.swapusage").unwrap();
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            &mut xsw as *mut _ as *mut libc::c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc == 0 {
        (xsw.xsu_used, xsw.xsu_total)
    } else {
        (0, 0)
    }
}

pub fn read() -> Option<MemDetail> {
    let stats = vm_stats()?;
    let page = sysctl_u64("hw.pagesize")?;
    let total = sysctl_u64("hw.memsize")?;

    let wired = stats.wire_count as u64 * page;
    let compressed = stats.compressor_page_count as u64 * page;
    let app = (stats.internal_page_count.saturating_sub(stats.purgeable_count)) as u64 * page;
    let used = app + wired + compressed;
    let (swap_used, swap_total) = swap();

    // kern.memorystatus_vm_pressure_level: 1=normal, 2=warn, 4=critical
    let pressure = match sysctl_u32("kern.memorystatus_vm_pressure_level") {
        Some(4) => PressureLevel::Critical,
        Some(2) => PressureLevel::Warn,
        _ => PressureLevel::Normal,
    };

    Some(MemDetail {
        app,
        wired,
        compressed,
        free: total.saturating_sub(used),
        swap_used,
        swap_total,
        pressure,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn plausible_memory_values() {
        let m = super::read().expect("memory read should work on macOS");
        let total = crate::mac::sysctl::sysctl_u64("hw.memsize").unwrap();
        assert!(m.app + m.wired + m.compressed <= total);
        assert!(m.wired > 0);
        assert!(m.swap_total >= m.swap_used);
    }
}
