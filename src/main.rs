mod types;

use crate::types::Metrics;
use chrono::{DateTime, Utc};
use log::info;
use reqwest;
use std::collections::VecDeque;
use std::time::Instant;
use std::{env, thread, time};
use sysinfo::{CpuExt, DiskExt, System, SystemExt};

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
        MetricHistory {
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
}

fn main() {
    env::set_var("RUST_LOG", "info");
    env_logger::init(); // Initialize the logger.

    let mut history = MetricHistory::new();
    let monitoring_interval = time::Duration::from_secs(5);

    loop {
        let mut sys = System::new_all();
        sys.refresh_all();

        let cpu_usage = sys.global_cpu_info().cpu_usage() as f64;
        let ram_usage = sys.used_memory() as f64 / sys.total_memory() as f64 * 100.0;
        let disk_usage = sys
            .disks()
            .iter()
            .map(|d| d.total_space() as f64 - d.available_space() as f64)
            .sum();
        let network_latency = check_network_latency("https://www.google.com");

        history.add(cpu_usage, ram_usage, disk_usage, network_latency);

        let status = analyze_status(&history);
        log_status(&status);

        thread::sleep(monitoring_interval);
    }
}

fn check_network_latency(url: &str) -> f64 {
    let start = Instant::now();
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();

    match client.get(url).send() {
        Ok(_) => start.elapsed().as_secs_f64(),
        Err(_) => f64::MAX,
    }
}


fn analyze_status(history: &MetricHistory) -> types::Metrics {
    let cpu_avg = history.cpu_usage.iter().sum::<f64>() / history.cpu_usage.len() as f64;
    let ram_avg = history.ram_usage.iter().sum::<f64>() / history.ram_usage.len() as f64;

    // Determine statuses
    let cpu_status = get_status(cpu_avg, 50.0, 70.0, 90.0);
    let ram_status = get_status(ram_avg, 40.0, 60.0, 80.0);
    let disk_status = types::MetricStatus::Normal; // Placeholder logic for disk
    let network_status = types::MetricStatus::Normal; // Placeholder logic for network

    types::Metrics {
        cpu: types::Metric {
            value: cpu_avg,
            status: cpu_status,
        },
        ram: types::Metric {
            value: ram_avg,
            status: ram_status,
        },
        disk: types::Metric {
            value: 0.0, // Placeholder value for disk
            status: disk_status,
        },
        network: types::Metric {
            value: 0.0, // Placeholder value for network
            status: network_status,
        },
    }
}

fn get_status(value: f64, slight: f64, abnormal: f64, critical: f64) -> types::MetricStatus {
    if value > critical {
        types::MetricStatus::Critical
    } else if value > abnormal {
        types::MetricStatus::Abnormal
    } else if value > slight {
        types::MetricStatus::SlightlyAbnormal
    } else {
        types::MetricStatus::Normal
    }
}


fn log_status(metrics: &Metrics) {
    let now = Utc::now();
    let json_output = serde_json::to_string(metrics).unwrap();
    println!("[{}] Metrics: {}", now, json_output);
    info!("[{}] Metrics: {}", now, json_output);
}


