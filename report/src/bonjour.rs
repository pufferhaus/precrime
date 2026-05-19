//! Bonjour/mDNS publisher via avahi-publish-service subprocess.

/// Spawn `avahi-publish-service` to advertise this REPORT instance on the LAN.
/// Returns the child process on success so the caller can kill it on shutdown.
/// Returns `None` on non-Linux targets or if the command is unavailable.
#[cfg(target_os = "linux")]
pub fn spawn_bonjour_publisher(report_name: &str, reg_port: u16) -> Option<std::process::Child> {
    std::process::Command::new("avahi-publish-service")
        .args([
            report_name,
            "_precrime-report._tcp",
            &reg_port.to_string(),
            "v=1",
            &format!("reg_port={reg_port}"),
        ])
        .spawn()
        .map_err(|e| tracing::warn!(error = ?e, "avahi-publish-service not available"))
        .ok()
}

#[cfg(not(target_os = "linux"))]
pub fn spawn_bonjour_publisher(_report_name: &str, _reg_port: u16) -> Option<std::process::Child> {
    None
}
