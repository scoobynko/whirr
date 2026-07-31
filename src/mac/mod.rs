pub mod battery;
pub mod hid_temp;
pub mod ioreport;
pub mod memory;
pub mod proc;
pub mod sysctl;

/// Whether this process is running on real Apple hardware, as opposed to a
/// virtualised Apple Silicon host such as a GitHub Actions `macos-14` runner.
///
/// Several readers in this module talk to physical sensors — the HID
/// temperature client, IOReport's "Energy Model" group, the `hw.perflevel*`
/// core-count sysctls. A VM is arm64 macOS and compiles everything fine, but
/// exposes none of them, so their tests cannot pass there. Rather than delete
/// those tests or let them fail every CI run, they skip when this returns
/// false — which keeps them strict on a real Mac, where the assertions are
/// the whole point.
///
/// Two independent signals, either of which is conclusive on its own:
///
/// - `kern.hv_vmm_present` is macOS's own hypervisor flag — 1 under a VM.
/// - `hw.nperflevels` is 2 on every Apple Silicon chip, which all have both
///   performance and efficiency cores. A virtualised host presents one uniform
///   core set and reports 1.
///
/// Both are checked because either alone could be wrong on a host neither of
/// us has seen. Note that `hw.optional.arm64` and `hw.perflevel0.physicalcpu`
/// are *not* usable here: both are present on a virtualised arm64 host too.
#[cfg(test)]
pub(crate) fn on_real_hardware() -> bool {
    let virtualised = sysctl::sysctl_u32("kern.hv_vmm_present").is_some_and(|v| v != 0);
    let has_p_and_e_cores = sysctl::sysctl_u32("hw.nperflevels").is_some_and(|n| n >= 2);
    !virtualised && has_p_and_e_cores
}

/// Skip the rest of a test when there is no physical sensor hardware to read.
/// Prints why, so a skipped run in CI is visible rather than silently green.
#[cfg(test)]
macro_rules! needs_real_hardware {
    () => {
        if !crate::mac::on_real_hardware() {
            eprintln!(
                "skipping {}: no physical sensors (virtualised host)",
                module_path!()
            );
            return;
        }
    };
}

#[cfg(test)]
pub(crate) use needs_real_hardware;
