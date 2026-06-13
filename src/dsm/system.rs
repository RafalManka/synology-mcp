use anyhow::Context;
use serde::Serialize;

use super::client::DsmClient;

#[derive(Serialize)]
pub struct SystemInfo {
    pub model: String,
    pub serial: String,
    pub dsm_version: String,
    pub uptime_secs: u64,
    pub hostname: String,
    pub temperature_c: Option<i64>,
    pub ram_mb: u64,
}

#[derive(Serialize)]
pub struct SystemUtilisation {
    pub cpu_percent: u64,
    pub memory_total_mb: u64,
    pub memory_used_mb: u64,
    pub memory_percent: u64,
    pub network_rx_kbps: u64,
    pub network_tx_kbps: u64,
}

#[derive(Serialize)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub version: String,
    pub status: String,
}

pub async fn get_system_info(client: &DsmClient) -> anyhow::Result<SystemInfo> {
    let data = client.call("SYNO.Core.System", 1, "info", &[]).await?;

    // up_time is "H:M:S" e.g. "2038:0:5"
    let uptime_secs = data["up_time"]
        .as_str()
        .and_then(|s| {
            let parts: Vec<u64> = s.split(':').filter_map(|p| p.parse().ok()).collect();
            if parts.len() == 3 {
                Some(parts[0] * 3600 + parts[1] * 60 + parts[2])
            } else {
                None
            }
        })
        .unwrap_or(0);

    // ram_size is integer MB
    let ram_mb = data["ram_size"]
        .as_u64()
        .or_else(|| data["ram_size"].as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0);

    Ok(SystemInfo {
        model: data["model"].as_str().unwrap_or("unknown").to_string(),
        serial: data["serial"].as_str().unwrap_or("unknown").to_string(),
        dsm_version: data["firmware_ver"].as_str().unwrap_or("unknown").to_string(),
        uptime_secs,
        hostname: data["hostname"].as_str().unwrap_or("DiskStation").to_string(),
        temperature_c: data["sys_temp"].as_i64(),
        ram_mb,
    })
}

pub async fn get_system_utilisation(client: &DsmClient) -> anyhow::Result<SystemUtilisation> {
    let data = client
        .call("SYNO.Core.System.Utilization", 1, "get", &[])
        .await?;

    let cpu_percent = data["cpu"]["user_load"].as_u64().unwrap_or(0)
        + data["cpu"]["system_load"].as_u64().unwrap_or(0);

    let memory_total_mb = data["memory"]["total_real"].as_u64().unwrap_or(0) / 1024;
    let avail_mb = data["memory"]["avail_real"].as_u64().unwrap_or(0) / 1024;
    let memory_used_mb = memory_total_mb.saturating_sub(avail_mb);
    let memory_percent = data["memory"]["real_usage"].as_u64().unwrap_or(0);

    // sum all network interfaces
    let (rx, tx) = data["network"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .fold((0u64, 0u64), |(rx, tx), iface| {
            (
                rx + iface["rx"].as_u64().unwrap_or(0),
                tx + iface["tx"].as_u64().unwrap_or(0),
            )
        });

    Ok(SystemUtilisation {
        cpu_percent,
        memory_total_mb,
        memory_used_mb,
        memory_percent,
        network_rx_kbps: rx / 1024,
        network_tx_kbps: tx / 1024,
    })
}

pub async fn list_packages(client: &DsmClient) -> anyhow::Result<Vec<Package>> {
    let data = client
        .call("SYNO.Core.Package", 1, "list", &[("additional", "description")])
        .await?;

    let packages = data["packages"]
        .as_array()
        .context("packages field missing")?
        .iter()
        .map(|p| Package {
            id: p["id"].as_str().unwrap_or("").to_string(),
            name: p["name"].as_str().unwrap_or("").to_string(),
            version: p["version"].as_str().unwrap_or("").to_string(),
            status: p["status"].as_str().unwrap_or("unknown").to_string(),
        })
        .collect();

    Ok(packages)
}
