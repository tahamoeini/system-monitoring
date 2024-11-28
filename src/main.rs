mod types;

use crate::types::{Metric, MetricStatus, Metrics};
use chrono::Utc;
use log::info;
use reqwest;
use serde::Serialize;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use std::{env, thread};
use sysinfo::{Components, Disks, Networks, System};

const MAX_HISTORY: usize = 100;

#[derive(Debug, Clone)]
struct MetricHistory {
    cpu_usage: VecDeque<f64>,
    ram_usage: VecDeque<f64>,
    disk_usage: VecDeque<f64>,
    network_latency: VecDeque<f64>,
}

impl MetricHistory {
    fn new() -> Self {
        Self {
            cpu_usage: VecDeque::with_capacity(MAX_HISTORY),
            ram_usage: VecDeque::with_capacity(MAX_HISTORY),
            disk_usage: VecDeque::with_capacity(MAX_HISTORY),
            network_latency: VecDeque::with_capacity(MAX_HISTORY),
        }
    }

    fn add(&mut self, cpu: f64, ram: f64, disk: f64, latency: f64) {
        if self.cpu_usage.len() == MAX_HISTORY {
            self.cpu_usage.pop_front();
            self.ram_usage.pop_front();
            self.disk_usage.pop_front();
            self.network_latency.pop_front();
        }
        self.cpu_usage.push_back(cpu);
        self.ram_usage.push_back(ram);
        self.disk_usage.push_back(disk);
        self.network_latency.push_back(latency);
    }

    fn z_score_anomaly(&self, data: &VecDeque<f64>, threshold: f64) -> bool {
        if data.len() < 2 {
            return false;
        }
        let mean: f64 = data.iter().sum::<f64>() / data.len() as f64;
        let stddev: f64 =
            (data.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / data.len() as f64).sqrt();
        let last_value = data.back().unwrap();
        let z_score = (last_value - mean) / stddev;
        z_score.abs() > threshold
    }
}

#[derive(Debug, Serialize)]
struct AnomalyStatus {
    cpu_anomaly: bool,
    ram_anomaly: bool,
    disk_anomaly: bool,
    network_anomaly: bool,
}

fn main() {
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    let mut history = MetricHistory::new();
    let monitoring_interval = Duration::from_secs(5);

    loop {
        let mut sys = System::new_all();
        sys.refresh_all();

        let cpu_usage = sys.global_cpu_usage() as f64;
        let ram_usage = sys.used_memory() as f64 / sys.total_memory() as f64 * 100.0;
        let disk_usage = Disks::new_with_refreshed_list()
            .iter()
            .map(|d| d.total_space() as f64 - d.available_space() as f64)
            .sum::<f64>();
        let network_latency = check_network_latency("https://www.google.com");

        history.add(cpu_usage, ram_usage, disk_usage, network_latency);

        let metrics = analyze_status(&history);
        let anomalies = detect_anomalies(&history);
        log_status(&metrics, &anomalies);

        thread::sleep(monitoring_interval);
    }
}

fn check_network_latency(url: &str) -> f64 {
    let start = Instant::now();
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();

    match client.get(url).send() {
        Ok(_) => start.elapsed().as_secs_f64(),
        Err(_) => f64::MAX,
    }
}

fn analyze_status(history: &MetricHistory) -> Metrics {
    let cpu_avg = history.cpu_usage.iter().sum::<f64>() / history.cpu_usage.len() as f64;
    let ram_avg = history.ram_usage.iter().sum::<f64>() / history.ram_usage.len() as f64;

    let cpu_status = get_status(cpu_avg, 50.0, 70.0, 90.0);
    let ram_status = get_status(ram_avg, 40.0, 60.0, 80.0);

    Metrics {
        cpu: Metric {
            value: cpu_avg,
            status: cpu_status,
        },
        ram: Metric {
            value: ram_avg,
            status: ram_status,
        },
        disk: Metric {
            value: 0.0,
            status: MetricStatus::Normal,
        },
        network: Metric {
            value: 0.0,
            status: MetricStatus::Normal,
        },
    }
}

fn detect_anomalies(history: &MetricHistory) -> AnomalyStatus {
    AnomalyStatus {
        cpu_anomaly: history.z_score_anomaly(&history.cpu_usage, 2.5),
        ram_anomaly: history.z_score_anomaly(&history.ram_usage, 2.5),
        disk_anomaly: history.z_score_anomaly(&history.disk_usage, 2.5),
        network_anomaly: history.z_score_anomaly(&history.network_latency, 2.5),
    }
}

fn get_status(value: f64, slight: f64, abnormal: f64, critical: f64) -> MetricStatus {
    match value {
        v if v > critical => MetricStatus::Critical,
        v if v > abnormal => MetricStatus::Abnormal,
        v if v > slight => MetricStatus::SlightlyAbnormal,
        _ => MetricStatus::Normal,
    }
}

fn log_status(metrics: &Metrics, anomalies: &AnomalyStatus) {
    let now = Utc::now();
    let metrics_json = serde_json::to_string(metrics).unwrap();
    let anomalies_json = serde_json::to_string(anomalies).unwrap();
    println!("[{}] Metrics: {}", now, metrics_json);
    println!("[{}] Anomalies: {}", now, anomalies_json);
    info!("[{}] Metrics: {}", now, metrics_json);
    info!("[{}] Anomalies: {}", now, anomalies_json);
}
