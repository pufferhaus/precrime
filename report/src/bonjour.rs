//! Bonjour/mDNS publisher via avahi-publish-service subprocess.
//! A watchdog thread respawns the process if avahi-daemon restarts and kills it.

use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
use std::time::Duration;

#[cfg(target_os = "linux")]
pub fn spawn_bonjour_publisher(report_name: &str, reg_port: u16, shutdown: Arc<AtomicBool>) {
    let name = report_name.to_owned();
    let port = reg_port.to_string();
    std::thread::Builder::new()
        .name("report-bonjour-watch".into())
        .spawn(move || {
            while !shutdown.load(Ordering::Relaxed) {
                let mut child = match std::process::Command::new("avahi-publish-service")
                    .args([&name, "_precrime-report._tcp", &port, "v=1", &format!("reg_port={port}")])
                    .spawn()
                {
                    Ok(c) => {
                        tracing::info!("bonjour: avahi-publish-service started");
                        c
                    }
                    Err(e) => {
                        tracing::warn!(error = ?e, "bonjour: avahi-publish-service unavailable; retrying in 5s");
                        std::thread::sleep(Duration::from_secs(5));
                        continue;
                    }
                };

                // Block until the child exits (avahi-daemon restart, crash, etc.)
                match child.wait() {
                    Ok(status) if shutdown.load(Ordering::Relaxed) => {
                        tracing::debug!("bonjour: avahi-publish-service exited on shutdown ({status})");
                        break;
                    }
                    Ok(status) => {
                        tracing::warn!("bonjour: avahi-publish-service exited ({status}); respawning in 2s");
                        std::thread::sleep(Duration::from_secs(2));
                    }
                    Err(e) => {
                        tracing::warn!(error = ?e, "bonjour: wait error; respawning in 2s");
                        std::thread::sleep(Duration::from_secs(2));
                    }
                }
            }

            tracing::debug!("bonjour watchdog exiting");
        })
        .ok();
}

#[cfg(not(target_os = "linux"))]
pub fn spawn_bonjour_publisher(_report_name: &str, _reg_port: u16, _shutdown: Arc<AtomicBool>) {}
