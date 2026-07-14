use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFMutableDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;

use crate::sampler::BatterySnap;

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOServiceMatching(name: *const libc::c_char) -> CFMutableDictionaryRef;
    fn IOServiceGetMatchingService(port: u32, matching: CFMutableDictionaryRef) -> u32;
    fn IORegistryEntryCreateCFProperties(
        entry: u32,
        props: *mut CFMutableDictionaryRef,
        allocator: *const libc::c_void,
        options: u32,
    ) -> i32;
    fn IOObjectRelease(obj: u32) -> i32;
}

fn get_i64(d: &CFDictionary<CFString, CFType>, key: &str) -> Option<i64> {
    d.find(CFString::new(key))
        .and_then(|v| v.downcast::<CFNumber>())
        .and_then(|n| n.to_i64())
}

pub fn read() -> Option<BatterySnap> {
    unsafe {
        let matching = IOServiceMatching(c"AppleSmartBattery".as_ptr());
        let service = IOServiceGetMatchingService(0, matching); // 0 = kIOMainPortDefault
        if service == 0 {
            return None;
        }
        let mut props: CFMutableDictionaryRef = std::ptr::null_mut();
        let rc = IORegistryEntryCreateCFProperties(service, &mut props, std::ptr::null(), 0);
        IOObjectRelease(service);
        if rc != 0 || props.is_null() {
            return None;
        }
        let dict: CFDictionary<CFString, CFType> =
            CFDictionary::wrap_under_create_rule(props as _);

        let percent = get_i64(&dict, "CurrentCapacity")? as u8; // % on Apple Silicon
        let cycles = get_i64(&dict, "CycleCount").unwrap_or(0) as u32;
        let charging = dict
            .find(CFString::new("IsCharging"))
            .and_then(|v| v.downcast::<CFBoolean>())
            .map(bool::from)
            .unwrap_or(false);
        let health_pct = match (
            get_i64(&dict, "NominalChargeCapacity"),
            get_i64(&dict, "DesignCapacity"),
        ) {
            (Some(n), Some(d)) if d > 0 => Some(((n * 100) / d).min(100) as u8),
            _ => None,
        };
        Some(BatterySnap { percent, charging, cycles, health_pct })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn battery_reads_or_is_absent() {
        // On a MacBook this must be Some with sane values; on a desktop, None.
        if let Some(b) = super::read() {
            assert!(b.percent <= 100);
            assert!(b.cycles < 10_000);
        }
    }
}
