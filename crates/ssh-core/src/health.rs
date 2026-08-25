//! Server health checks: TCP reachability + metrics over an open SSH session.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::model::{now_secs, HealthReport, HostEntry};
use crate::session::SshSession;

/// One combined remote command; sections split by a sentinel so a single
/// round-trip collects everything.
const PROBE_CMD: &str = "uptime 2>/dev/null; echo @@; cat /proc/loadavg 2>/dev/null; echo @@; free -b 2>/dev/null | awk 'NR==2{print $2, $3}'; echo @@; df -P / 2>/dev/null | awk 'NR==2{print $5}'";

/// TCP reachability + latency only (no credentials needed).
pub async fn check_tcp(host: &HostEntry) -> HealthReport {
    let started = Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(6),
        tokio::net::TcpStream::connect((host.host.as_str(), host.port)),
    )
    .await;
    let mut report = HealthReport {
        host_id: host.id.clone(),
        timestamp: now_secs(),
        reachable: false,
        latency_ms: None,
        ssh_ok: false,
        uptime: None,
        load_avg: None,
        mem_used_pct: None,
        disk_used_pct: None,
        error: None,
    };
    match result {
        Ok(Ok(_stream)) => {
            report.reachable = true;
            report.latency_ms = Some(started.elapsed().as_millis() as u64);
        }
        Ok(Err(e)) => report.error = Some(format!("connect failed: {e}")),
        Err(_) => report.error = Some("connect timed out".into()),
    }
    report
}

/// Full check: TCP + metrics through an already-open session (if provided).
pub async fn check(host: &HostEntry, session: Option<Arc<SshSession>>) -> HealthReport {
    let mut report = check_tcp(host).await;
    if !report.reachable {
        return report;
    }
    let Some(session) = session else {
        return report;
    };
    match session.exec(PROBE_CMD).await {
        Ok((_code, output)) => {
            report.ssh_ok = true;
            parse_probe(&output, &mut report);
        }
        Err(e) => {
            report.error = Some(format!("probe failed: {e}"));
        }
    }
    report
}

/// Parses the four `@@`-separated probe sections into the report.
pub fn parse_probe(output: &str, report: &mut HealthReport) {
    let sections: Vec<&str> = output.split("@@").collect();
    if let Some(uptime) = sections.first() {
        let uptime = uptime.trim();
        if !uptime.is_empty() {
            report.uptime = Some(uptime.to_string());
        }
    }
    if let Some(loadavg) = sections.get(1) {
        let parts: Vec<&str> = loadavg.split_whitespace().take(3).collect();
        if parts.len() == 3 {
            report.load_avg = Some(parts.join(" "));
        }
    }
    if let Some(mem) = sections.get(2) {
        let nums: Vec<f64> = mem
            .split_whitespace()
            .filter_map(|t| t.parse().ok())
            .collect();
        if nums.len() >= 2 && nums[0] > 0.0 {
            report.mem_used_pct = Some((nums[1] / nums[0] * 100.0).clamp(0.0, 100.0));
        }
    }
    if let Some(disk) = sections.get(3) {
        if let Some(pct) = disk.trim().strip_suffix('%').and_then(|p| p.parse::<f64>().ok()) {
            report.disk_used_pct = Some(pct);
        }
    }
}
