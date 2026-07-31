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
/// `hw.optional.arm64` is set on any arm64 macOS including a VM, so it can't
/// be used here. The perflevel sysctls describe a physical P/E core split and
/// are absent under virtualisation, which is exactly the distinction needed.
#[cfg(test)]
pub(crate) fn on_real_hardware() -> bool {
    sysctl::sysctl_u32("hw.perflevel0.physicalcpu").is_some_and(|n| n > 0)
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
