use std::time::Instant;

use core_foundation::array::CFArray;
use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef, CFMutableDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};

use crate::sampler::PowerSnap;

#[repr(C)]
struct IOReportSubscription {
    _private: [u8; 0],
}
type IOReportSubscriptionRef = *mut IOReportSubscription;

#[link(name = "IOReport", kind = "dylib")]
extern "C" {
    fn IOReportCopyChannelsInGroup(
        group: CFStringRef,
        subgroup: CFStringRef,
        a: u64,
        b: u64,
        c: u64,
    ) -> CFMutableDictionaryRef;
    fn IOReportCreateSubscription(
        allocator: *const libc::c_void,
        channels: CFMutableDictionaryRef,
        subscribed: *mut CFMutableDictionaryRef,
        channel_id: u64,
        options: CFTypeRef,
    ) -> IOReportSubscriptionRef;
    fn IOReportCreateSamples(
        subscription: IOReportSubscriptionRef,
        channels: CFMutableDictionaryRef,
        options: CFTypeRef,
    ) -> CFDictionaryRef;
    fn IOReportCreateSamplesDelta(
        prev: CFDictionaryRef,
        current: CFDictionaryRef,
        options: CFTypeRef,
    ) -> CFDictionaryRef;
    fn IOReportChannelGetChannelName(item: CFDictionaryRef) -> CFStringRef;
    fn IOReportChannelGetUnitLabel(item: CFDictionaryRef) -> CFStringRef;
    fn IOReportSimpleGetIntegerValue(item: CFDictionaryRef, index: i32) -> i64;
}

pub struct PowerSampler {
    subscription: IOReportSubscriptionRef,
    channels: CFMutableDictionaryRef,
    prev: Option<(CFDictionaryRef, Instant)>,
}

// The subscription is only touched from the medium sampler thread.
unsafe impl Send for PowerSampler {}

fn cfstr_to_string(s: CFStringRef) -> String {
    if s.is_null() {
        return String::new();
    }
    unsafe { CFString::wrap_under_get_rule(s) }.to_string()
}

fn unit_to_joules(unit: &str) -> f64 {
    match unit.trim() {
        "mJ" => 1e-3,
        "uJ" | "µJ" => 1e-6,
        "nJ" => 1e-9,
        _ => 1e-9,
    }
}

impl PowerSampler {
    pub fn new() -> Option<Self> {
        unsafe {
            let group = CFString::new("Energy Model");
            let channels = IOReportCopyChannelsInGroup(
                group.as_concrete_TypeRef(),
                std::ptr::null(),
                0,
                0,
                0,
            );
            if channels.is_null() {
                return None;
            }
            let mut subscribed: CFMutableDictionaryRef = std::ptr::null_mut();
            let subscription = IOReportCreateSubscription(
                std::ptr::null(),
                channels,
                &mut subscribed,
                0,
                std::ptr::null(),
            );
            if subscription.is_null() {
                return None;
            }
            Some(Self { subscription, channels, prev: None })
        }
    }

    fn for_each_channel(sample: CFDictionaryRef, mut f: impl FnMut(CFDictionaryRef)) {
        unsafe {
            let dict: CFDictionary = CFDictionary::wrap_under_get_rule(sample);
            let key = CFString::new("IOReportChannels");
            if let Some(arr) = dict.find(key.as_concrete_TypeRef() as *const _) {
                let arr: CFArray = CFArray::wrap_under_get_rule(*arr as _);
                for i in 0..arr.len() {
                    if let Some(item) = arr.get(i) {
                        f(*item as CFDictionaryRef);
                    }
                }
            }
        }
    }

    pub fn sample(&mut self) -> Option<PowerSnap> {
        unsafe {
            let now = Instant::now();
            let cur = IOReportCreateSamples(self.subscription, self.channels, std::ptr::null());
            if cur.is_null() {
                return None;
            }
            let result = match self.prev {
                None => None,
                Some((prev, t0)) => {
                    let dt = now.duration_since(t0).as_secs_f64().max(0.001);
                    let delta = IOReportCreateSamplesDelta(prev, cur, std::ptr::null());
                    let mut snap = PowerSnap { cpu_w: 0.0, gpu_w: 0.0, ane_w: 0.0 };
                    if !delta.is_null() {
                        Self::for_each_channel(delta, |item| {
                            let name = cfstr_to_string(IOReportChannelGetChannelName(item));
                            let unit = cfstr_to_string(IOReportChannelGetUnitLabel(item));
                            let joules = IOReportSimpleGetIntegerValue(item, 0) as f64
                                * unit_to_joules(&unit);
                            let watts = joules / dt;
                            // The "Energy Model" group also exposes per-core/per-cluster/
                            // per-DVFS-state breakdown channels (e.g. "ECPU0", "PCPU",
                            // "ECPUDTL07") whose energy is already folded into the
                            // aggregate "CPU Energy"/"GPU Energy"/"ANE" channels below.
                            // Matching on a broad `contains` substring (e.g. "CPU") would
                            // sum those breakdown channels on top of the aggregate and
                            // wildly overcount (~40W instead of ~10W at idle on M-series).
                            // Match the exact aggregate channel names macmon uses instead.
                            if name == "GPU Energy" {
                                snap.gpu_w += watts;
                            } else if name.ends_with("CPU Energy") {
                                // "CPU Energy" on Basic/Max dies, "DIE_{n}_CPU Energy" on Ultra.
                                snap.cpu_w += watts;
                            } else if name.starts_with("ANE") {
                                // "ANE" on Basic, "ANE0" on Max, "ANE0_{n}" on Ultra.
                                snap.ane_w += watts;
                            }
                        });
                        CFRelease(delta as CFTypeRef);
                    }
                    Some(snap)
                }
            };
            if let Some((prev, _)) = self.prev.take() {
                CFRelease(prev as CFTypeRef);
            }
            self.prev = Some((cur, now));
            result
        }
    }

    pub fn channel_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        unsafe {
            let cur = IOReportCreateSamples(self.subscription, self.channels, std::ptr::null());
            if !cur.is_null() {
                Self::for_each_channel(cur, |item| {
                    names.push(cfstr_to_string(IOReportChannelGetChannelName(item)));
                });
                CFRelease(cur as CFTypeRef);
            }
        }
        names
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn power_sampler_returns_plausible_watts() {
        let mut p =
            super::PowerSampler::new().expect("Energy Model group must exist on Apple Silicon");
        assert!(p.sample().is_none()); // first call primes
        std::thread::sleep(std::time::Duration::from_millis(500));
        let s = p.sample().expect("second sample");
        assert!(s.cpu_w >= 0.0 && s.cpu_w < 200.0);
    }
}
