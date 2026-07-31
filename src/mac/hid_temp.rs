use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{
    kCFAllocatorDefault, CFAllocatorRef, CFGetTypeID, CFRelease, CFTypeRef, TCFType,
};
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};

type IOHIDEventSystemClientRef = *mut libc::c_void;
type IOHIDServiceClientRef = *mut libc::c_void;
type IOHIDEventRef = *mut libc::c_void;

// AppleVendor HID page: temperature sensors
const K_HID_PAGE_APPLE_VENDOR: i64 = 0xff00;
const K_HID_USAGE_TEMPERATURE_SENSOR: i64 = 5;
const K_IOHID_EVENT_TYPE_TEMPERATURE: i64 = 15;

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOHIDEventSystemClientCreate(allocator: CFAllocatorRef) -> IOHIDEventSystemClientRef;
    fn IOHIDEventSystemClientSetMatching(
        client: IOHIDEventSystemClientRef,
        matching: CFDictionaryRef,
    );
    fn IOHIDEventSystemClientCopyServices(client: IOHIDEventSystemClientRef) -> CFArrayRef;
    fn IOHIDServiceClientCopyProperty(
        service: IOHIDServiceClientRef,
        key: CFStringRef,
    ) -> CFTypeRef;
    fn IOHIDServiceClientCopyEvent(
        service: IOHIDServiceClientRef,
        event_type: i64,
        options: i32,
        timestamp: i64,
    ) -> IOHIDEventRef;
    fn IOHIDEventGetFloatValue(event: IOHIDEventRef, field: i32) -> f64;
}

// Teardown policy: `TempSensor` is a process-lifetime singleton (Task 9 holds
// it in the medium sampler for the whole run). The `client` handle is a
// Create-rule object intentionally leaked at process exit — a bounded,
// one-time leak the OS reclaims. We deliberately do NOT implement Drop:
// IOHIDEventSystemClient is a private API and releasing it buys nothing at
// exit while a wrong release would be worse than the bounded leak.
pub struct TempSensor {
    client: IOHIDEventSystemClientRef,
}

// The client is only touched from the medium sampler thread; opaque handle,
// no shared mutation.
unsafe impl Send for TempSensor {}

impl TempSensor {
    pub fn new() -> Option<Self> {
        unsafe {
            let client = IOHIDEventSystemClientCreate(kCFAllocatorDefault);
            if client.is_null() {
                return None;
            }
            let matching = CFDictionary::from_CFType_pairs(&[
                (
                    CFString::new("PrimaryUsagePage").as_CFType(),
                    CFNumber::from(K_HID_PAGE_APPLE_VENDOR).as_CFType(),
                ),
                (
                    CFString::new("PrimaryUsage").as_CFType(),
                    CFNumber::from(K_HID_USAGE_TEMPERATURE_SENSOR).as_CFType(),
                ),
            ]);
            IOHIDEventSystemClientSetMatching(client, matching.as_concrete_TypeRef() as _);
            Some(Self { client })
        }
    }

    pub fn list(&self) -> Vec<(String, f32)> {
        let mut out = Vec::new();
        unsafe {
            let services = IOHIDEventSystemClientCopyServices(self.client);
            if services.is_null() {
                return out;
            }
            let arr: CFArray = CFArray::wrap_under_create_rule(services);
            let product_key = CFString::new("Product");
            let field = (K_IOHID_EVENT_TYPE_TEMPERATURE << 16) as i32;
            for i in 0..arr.len() {
                let Some(svc) = arr.get(i) else { continue };
                let svc = *svc as IOHIDServiceClientRef;
                let name_ref =
                    IOHIDServiceClientCopyProperty(svc, product_key.as_concrete_TypeRef());
                if name_ref.is_null() {
                    continue;
                }
                if CFGetTypeID(name_ref) != CFString::type_id() {
                    // Not a CFString; release the Create-rule ref and skip.
                    CFRelease(name_ref);
                    continue;
                }
                let name = CFString::wrap_under_create_rule(name_ref as _).to_string();
                let event = IOHIDServiceClientCopyEvent(svc, K_IOHID_EVENT_TYPE_TEMPERATURE, 0, 0);
                if event.is_null() {
                    continue;
                }
                let value = IOHIDEventGetFloatValue(event, field) as f32;
                CFRelease(event as CFTypeRef);
                if value > -40.0 && value < 150.0 {
                    out.push((name, value));
                }
            }
        }
        out
    }

    /// Mean of CPU die sensors ("tdie" on Apple Silicon PMU), falling back to
    /// any sensor whose name mentions the CPU, else None.
    pub fn read(&self) -> Option<f32> {
        let all = self.list();
        let pick = |pred: &dyn Fn(&str) -> bool| -> Vec<f32> {
            all.iter()
                .filter(|(n, _)| pred(&n.to_lowercase()))
                .map(|&(_, v)| v)
                .collect()
        };
        let mut vals = pick(&|n| n.contains("tdie"));
        if vals.is_empty() {
            vals = pick(&|n| n.contains("cpu"));
        }
        if vals.is_empty() {
            return None;
        }
        Some(vals.iter().sum::<f32>() / vals.len() as f32)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn cpu_temp_plausible() {
        crate::mac::needs_real_hardware!();
        let t = super::TempSensor::new().expect("HID client");
        let c = t.read().expect("at least one CPU temp sensor");
        assert!((10.0..120.0).contains(&c), "got {c}");
    }
}
