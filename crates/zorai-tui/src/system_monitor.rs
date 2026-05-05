use std::fs;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SystemMonitorDisplay {
    pub(crate) cpu_percent: f64,
    pub(crate) memory_used_bytes: u64,
    pub(crate) memory_total_bytes: u64,
    pub(crate) gpu_percent: Option<f64>,
    pub(crate) gpu_memory_used_bytes: Option<u64>,
    pub(crate) gpu_memory_total_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CpuTimes {
    idle: u64,
    total: u64,
}

#[derive(Debug, Default)]
pub(crate) struct SystemMonitorSampler {
    last_cpu: Option<CpuTimes>,
}

impl SystemMonitorSampler {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn sample(&mut self) -> Option<SystemMonitorDisplay> {
        let memory = read_memory_usage()?;
        let cpu_percent = self.cpu_percent();
        let gpu = read_first_gpu_usage();
        Some(SystemMonitorDisplay {
            cpu_percent,
            memory_used_bytes: memory.used_bytes,
            memory_total_bytes: memory.total_bytes,
            gpu_percent: gpu.map(|usage| usage.percent),
            gpu_memory_used_bytes: gpu.map(|usage| usage.memory_used_bytes),
            gpu_memory_total_bytes: gpu.map(|usage| usage.memory_total_bytes),
        })
    }

    fn cpu_percent(&mut self) -> f64 {
        let Some(current) = read_cpu_times() else {
            return 0.0;
        };
        let percent = self
            .last_cpu
            .and_then(|previous| cpu_percent_between(previous, current))
            .unwrap_or(0.0);
        self.last_cpu = Some(current);
        percent
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MemoryUsage {
    used_bytes: u64,
    total_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct GpuUsage {
    percent: f64,
    memory_used_bytes: u64,
    memory_total_bytes: u64,
}

fn read_cpu_times() -> Option<CpuTimes> {
    let stat = fs::read_to_string("/proc/stat").ok()?;
    parse_cpu_times(&stat)
}

fn parse_cpu_times(stat: &str) -> Option<CpuTimes> {
    let line = stat.lines().find(|line| line.starts_with("cpu "))?;
    let values: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|part| part.parse::<u64>().ok())
        .collect();
    if values.len() < 4 {
        return None;
    }

    let idle = values
        .get(3)
        .copied()
        .unwrap_or(0)
        .saturating_add(values.get(4).copied().unwrap_or(0));
    let total = values.iter().copied().sum();
    Some(CpuTimes { idle, total })
}

fn cpu_percent_between(previous: CpuTimes, current: CpuTimes) -> Option<f64> {
    let total_delta = current.total.checked_sub(previous.total)?;
    let idle_delta = current.idle.checked_sub(previous.idle)?;
    if total_delta == 0 || idle_delta > total_delta {
        return None;
    }
    let busy = total_delta - idle_delta;
    Some(((busy as f64 / total_delta as f64) * 100.0).clamp(0.0, 100.0))
}

fn read_memory_usage() -> Option<MemoryUsage> {
    let meminfo = fs::read_to_string("/proc/meminfo").ok()?;
    parse_memory_usage(&meminfo)
}

fn parse_memory_usage(meminfo: &str) -> Option<MemoryUsage> {
    let mut total_kib = None;
    let mut available_kib = None;
    let mut free_kib = None;

    for line in meminfo.lines() {
        let mut parts = line.split_whitespace();
        let key = parts.next()?.trim_end_matches(':');
        let value = parts.next().and_then(|part| part.parse::<u64>().ok());
        match key {
            "MemTotal" => total_kib = value,
            "MemAvailable" => available_kib = value,
            "MemFree" => free_kib = value,
            _ => {}
        }
    }

    let total_bytes = total_kib?.saturating_mul(1024);
    let available_bytes = available_kib.or(free_kib).unwrap_or(0).saturating_mul(1024);
    Some(MemoryUsage {
        used_bytes: total_bytes.saturating_sub(available_bytes),
        total_bytes,
    })
}

fn read_first_gpu_usage() -> Option<GpuUsage> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    parse_first_gpu_usage(&stdout)
}

fn parse_first_gpu_usage(stdout: &str) -> Option<GpuUsage> {
    stdout.lines().find_map(|line| {
        let mut parts = line.split(',').map(str::trim);
        let percent = parts.next()?.parse::<f64>().ok()?.clamp(0.0, 100.0);
        let used_mib = parts.next()?.parse::<u64>().ok()?;
        let total_mib = parts.next()?.parse::<u64>().ok()?;
        Some(GpuUsage {
            percent,
            memory_used_bytes: used_mib.saturating_mul(1024 * 1024),
            memory_total_bytes: total_mib.saturating_mul(1024 * 1024),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cpu_times_reads_idle_and_total() {
        let times = parse_cpu_times("cpu  10 20 30 40 5 6 7 8 0 0\n").unwrap();

        assert_eq!(times.idle, 45);
        assert_eq!(times.total, 126);
    }

    #[test]
    fn cpu_percent_between_uses_busy_delta() {
        let percent = cpu_percent_between(
            CpuTimes {
                idle: 40,
                total: 100,
            },
            CpuTimes {
                idle: 70,
                total: 200,
            },
        )
        .unwrap();

        assert_eq!(percent, 70.0);
    }

    #[test]
    fn parse_memory_usage_uses_available_memory() {
        let usage = parse_memory_usage(
            "MemTotal:       2048 kB\nMemFree:        256 kB\nMemAvailable:   512 kB\n",
        )
        .unwrap();

        assert_eq!(usage.total_bytes, 2048 * 1024);
        assert_eq!(usage.used_bytes, 1536 * 1024);
    }

    #[test]
    fn parse_first_gpu_usage_reads_utilization_and_vram() {
        let usage = parse_first_gpu_usage("18, 3277, 24576\n").unwrap();

        assert_eq!(usage.percent, 18.0);
        assert_eq!(usage.memory_used_bytes, 3277 * 1024 * 1024);
        assert_eq!(usage.memory_total_bytes, 24576 * 1024 * 1024);
    }
}
