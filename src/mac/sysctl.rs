use std::ffi::CString;

pub fn sysctl_raw(name: &str, buf: &mut [u8]) -> Option<usize> {
    let cname = CString::new(name).ok()?;
    let mut len = buf.len();
    let rc = unsafe {
        libc::sysctlbyname(
            cname.as_ptr(),
            buf.as_mut_ptr() as *mut libc::c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    (rc == 0).then_some(len)
}

pub fn sysctl_string(name: &str) -> Option<String> {
    let mut buf = [0u8; 256];
    let len = sysctl_raw(name, &mut buf)?.min(buf.len());
    let end = buf[..len].iter().position(|&b| b == 0).unwrap_or(len);
    String::from_utf8(buf[..end].to_vec()).ok()
}

pub fn sysctl_u32(name: &str) -> Option<u32> {
    let mut buf = [0u8; 4];
    (sysctl_raw(name, &mut buf)? == 4).then(|| u32::from_ne_bytes(buf))
}

pub fn sysctl_u64(name: &str) -> Option<u64> {
    let mut buf = [0u8; 8];
    match sysctl_raw(name, &mut buf)? {
        8 => Some(u64::from_ne_bytes(buf)),
        4 => Some(u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]) as u64),
        _ => None,
    }
}

pub struct SystemStatic {
    pub chip: String,
    pub os_version: String,
}

impl SystemStatic {
    pub fn read() -> Self {
        Self {
            chip: sysctl_string("machdep.cpu.brand_string").unwrap_or_else(|| "Mac".into()),
            os_version: sysctl_string("kern.osproductversion").unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_real_values() {
        let s = SystemStatic::read();
        assert!(s.chip.contains("Apple"));
        assert!(!s.os_version.is_empty());
    }
}
