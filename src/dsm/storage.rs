use anyhow::Context;
use serde::Serialize;

use super::client::DsmClient;

#[derive(Serialize)]
pub struct Volume {
    pub id: String,
    pub total_gb: f64,
    pub used_gb: f64,
    pub free_gb: f64,
    pub percent_used: f64,
    pub status: String,
    pub fs_type: String,
}

#[derive(Serialize)]
pub struct Disk {
    pub id: String,
    pub model: String,
    pub serial: String,
    pub temp_c: Option<i64>,
    pub status: String,
    pub smart_status: String,
    pub size_gb: f64,
    pub location: Option<u64>,
    pub disk_type: String,
}

pub async fn get_volumes(client: &DsmClient) -> anyhow::Result<Vec<Volume>> {
    let data = client
        .call("SYNO.Storage.CGI.Storage", 1, "load_info", &[])
        .await?;

    let volumes = data["volumes"]
        .as_array()
        .context("volumes field missing")?
        .iter()
        .map(|v| {
            let total: f64 = v["total_size"]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);
            let used: f64 = v["used_size"]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);
            let gb = 1_000_000_000.0_f64;
            let total_gb = total / gb;
            let used_gb = used / gb;
            let free_gb = (total - used) / gb;
            let percent_used = if total > 0.0 { used / total * 100.0 } else { 0.0 };

            Volume {
                id: v["id"].as_str().unwrap_or("").to_string(),
                total_gb: (total_gb * 100.0).round() / 100.0,
                used_gb: (used_gb * 100.0).round() / 100.0,
                free_gb: (free_gb * 100.0).round() / 100.0,
                percent_used: (percent_used * 10.0).round() / 10.0,
                status: v["status"].as_str().unwrap_or("unknown").to_string(),
                fs_type: v["fs_type"].as_str().unwrap_or("unknown").to_string(),
            }
        })
        .collect();

    Ok(volumes)
}

pub async fn get_disks(client: &DsmClient) -> anyhow::Result<Vec<Disk>> {
    let data = client
        .call("SYNO.Storage.CGI.Storage", 1, "load_info", &[])
        .await?;

    let disks = data["disks"]
        .as_array()
        .context("disks field missing")?
        .iter()
        .map(|d| {
            let size: f64 = d["size_total"]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);

            Disk {
                id: d["id"].as_str().unwrap_or("").to_string(),
                model: d["model"].as_str().unwrap_or("unknown").to_string(),
                serial: d["serial"].as_str().unwrap_or("unknown").to_string(),
                temp_c: d["temp"].as_i64(),
                status: d["status"].as_str().unwrap_or("unknown").to_string(),
                smart_status: d["smart_status"].as_str().unwrap_or("unknown").to_string(),
                size_gb: (size / 1_000_000_000.0 * 100.0).round() / 100.0,
                location: d["location"].as_u64(),
                disk_type: d["diskType"].as_str().unwrap_or("unknown").to_string(),
            }
        })
        .collect();

    Ok(disks)
}
