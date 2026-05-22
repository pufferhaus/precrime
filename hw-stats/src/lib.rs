pub struct CpuSnapshot {
    pub idle: u64,
    pub total: u64,
}

impl Default for CpuSnapshot {
    fn default() -> Self {
        Self { idle: 0, total: 0 }
    }
}

impl Clone for CpuSnapshot {
    fn clone(&self) -> Self {
        Self { idle: self.idle, total: self.total }
    }
}

// ── Public read API ──────────────────────────────────────────────────────────

pub fn read_cpu_temp_mc() -> Option<u32> {
    let s = std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp").ok()?;
    parse_cpu_temp_mc(&s)
}

pub fn read_cpu_load_pct(prev: &CpuSnapshot) -> (u8, CpuSnapshot) {
    let content = std::fs::read_to_string("/proc/stat").ok();
    let current = content
        .as_deref()
        .and_then(parse_cpu_snapshot)
        .unwrap_or(CpuSnapshot {
            idle: prev.idle,
            total: prev.total.saturating_add(1),
        });
    compute_load(prev, &current)
}

pub fn read_mem_mb() -> Option<(u32, u32)> {
    let s = std::fs::read_to_string("/proc/meminfo").ok()?;
    parse_mem_mb(&s)
}

pub fn read_wifi_rssi_dbm() -> Option<i16> {
    let s = std::fs::read_to_string("/proc/net/wireless").ok()?;
    parse_wifi_rssi(&s)
}

// ── Parsers (pub(crate) for testing) ─────────────────────────────────────────

pub(crate) fn parse_cpu_temp_mc(s: &str) -> Option<u32> {
    s.trim().parse().ok()
}

pub(crate) fn parse_cpu_snapshot(s: &str) -> Option<CpuSnapshot> {
    let line = s.lines().find(|l| l.starts_with("cpu "))?;
    let fields: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|t| t.parse().ok())
        .collect();
    let idle = *fields.get(3)?;
    let total: u64 = fields.iter().sum();
    Some(CpuSnapshot { idle, total })
}

pub(crate) fn compute_load(prev: &CpuSnapshot, current: &CpuSnapshot) -> (u8, CpuSnapshot) {
    let d_total = current.total.saturating_sub(prev.total);
    let d_idle = current.idle.saturating_sub(prev.idle);
    let load = if d_total == 0 {
        0u8
    } else {
        ((100 * (d_total - d_idle)) / d_total).min(100) as u8
    };
    (load, current.clone())
}

pub(crate) fn parse_mem_mb(s: &str) -> Option<(u32, u32)> {
    let mut total_kb: Option<u64> = None;
    let mut avail_kb: Option<u64> = None;
    for line in s.lines() {
        if line.starts_with("MemTotal:") {
            total_kb = line.split_whitespace().nth(1).and_then(|v| v.parse().ok());
        } else if line.starts_with("MemAvailable:") {
            avail_kb = line.split_whitespace().nth(1).and_then(|v| v.parse().ok());
        }
        if total_kb.is_some() && avail_kb.is_some() {
            break;
        }
    }
    let t = total_kb?;
    let a = avail_kb?;
    Some(((t - a) as u32 / 1024, t as u32 / 1024))
}

pub(crate) fn parse_wifi_rssi(s: &str) -> Option<i16> {
    // /proc/net/wireless has 2 header lines then data lines:
    // " wlan0: 0000   60.  -51.  -256. ..."
    // tokens[0]=iface, [1]=status, [2]=link, [3]=level (signal dBm)
    let line = s.lines().nth(2)?;
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let raw: i32 = tokens.get(3)?.trim_end_matches('.').parse().ok()?;
    // Old kernels report unsigned 0–255 where actual_dBm = raw - 256 for raw > 0
    let dbm = if raw > 0 { raw - 256 } else { raw };
    Some(dbm as i16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cpu_temp_mc_parses_millidegrees() {
        assert_eq!(parse_cpu_temp_mc("42300\n"), Some(42300));
        assert_eq!(parse_cpu_temp_mc("  51000  "), Some(51000));
        assert_eq!(parse_cpu_temp_mc("not_a_number\n"), None);
    }

    #[test]
    fn parse_cpu_snapshot_extracts_idle_and_total() {
        let proc_stat = "cpu  100 20 30 200 10 5 5 0 0 0\n";
        // fields: user=100 nice=20 system=30 idle=200 iowait=10 irq=5 softirq=5 ...
        // total = 100+20+30+200+10+5+5 = 370, idle = 200
        let snap = parse_cpu_snapshot(proc_stat).unwrap();
        assert_eq!(snap.idle, 200);
        assert_eq!(snap.total, 370);
    }

    #[test]
    fn compute_load_calculates_correct_percentage() {
        let prev = CpuSnapshot { idle: 200, total: 370 };
        // After: idle+20, total+100 → d_total=100, d_idle=20, busy=80 → 80%
        let current = CpuSnapshot { idle: 220, total: 470 };
        let (pct, _) = compute_load(&prev, &current);
        assert_eq!(pct, 80);
    }

    #[test]
    fn compute_load_zero_when_no_delta() {
        let snap = CpuSnapshot { idle: 100, total: 200 };
        let (pct, _) = compute_load(&snap, &snap.clone());
        assert_eq!(pct, 0);
    }

    #[test]
    fn parse_mem_mb_extracts_used_and_total() {
        let meminfo = "\
MemTotal:        2048000 kB\n\
MemFree:          512000 kB\n\
MemAvailable:     768000 kB\n\
Buffers:           64000 kB\n";
        // total=2048000 kB=2000 MB, avail=768000 kB=750 MB, used=2000-750=1250 MB
        let (used, total) = parse_mem_mb(meminfo).unwrap();
        assert_eq!(total, 2000);
        assert_eq!(used, 1250);
    }

    #[test]
    fn parse_mem_mb_returns_none_on_missing_fields() {
        assert!(parse_mem_mb("MemTotal: 1000 kB\n").is_none()); // no MemAvailable
    }

    #[test]
    fn parse_wifi_rssi_extracts_signal_level() {
        let wireless = "\
Inter-| sta-|   Quality\n\
 face | tus | link level\n\
 wlan0: 0000   60.  -54.  -256.        0\n";
        assert_eq!(parse_wifi_rssi(wireless), Some(-54));
    }

    #[test]
    fn parse_wifi_rssi_normalises_old_kernel_unsigned() {
        // Old kernels: 202 means 202 - 256 = -54 dBm
        let wireless = "\
Inter-| sta-|\n\
 face | tus |\n\
 wlan0: 0000   60.  202.  0.\n";
        assert_eq!(parse_wifi_rssi(wireless), Some(-54));
    }

    #[test]
    fn parse_wifi_rssi_returns_none_on_missing_data() {
        assert!(parse_wifi_rssi("no data here\n").is_none());
    }
}
