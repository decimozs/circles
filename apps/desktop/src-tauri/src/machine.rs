//! The name of this computer, as shown in macOS System Settings.

/// `scutil --get ComputerName` ("Marlon's MacBook Pro"), then the hostname, then `This Mac`.
pub fn machine_name() -> String {
    let from_scutil = std::process::Command::new("/usr/sbin/scutil")
        .args(["--get", "ComputerName"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|n| !n.is_empty());
    if let Some(name) = from_scutil {
        return name;
    }

    let mut buf = [0u8; 256];
    // SAFETY: `buf` is valid for `buf.len()` bytes and gethostname NUL-terminates within it.
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
    if rc == 0 {
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        let host = String::from_utf8_lossy(&buf[..end]).trim().to_string();
        if !host.is_empty() {
            return host;
        }
    }
    "This Mac".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_returns_a_name() {
        assert!(!machine_name().trim().is_empty());
    }
}
