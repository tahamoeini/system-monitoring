mod types;

use crate::types::{Metric, MetricStatus, Metrics};
use chrono::Utc;
use env_logger;
use log::info;
use reqwest;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use std::{env, thread};
use sysinfo::{Components, Disks, Networks, System};

// Constants
const MAX_HISTORY: usize = 100; // Maximum number of data points to keep in history
const MONITORING_INTERVAL: Duration = Duration::from_secs(5); // Monitoring interval in seconds
const ANOMALY_THRESHOLD: f64 = 2.5; // Z-score threshold for anomaly detection

// Struct to hold historical metrics data
#[derive(Debug, Clone)]
struct MetricHistory {
    data: VecDeque<[f64; 4]>, // Each data point contains [CPU, RAM, Disk, Network Latency]
}

impl MetricHistory {
    fn new() -> Self {
        Self {
            data: VecDeque::with_capacity(MAX_HISTORY),
        }
    }

    fn add(&mut self, cpu: f64, ram: f64, disk: f64, latency: f64) {
        // Maintain the history size
        if self.data.len() == MAX_HISTORY {
            self.data.pop_front();
        }
        self.data.push_back([cpu, ram, disk, latency]);
    }

    /// Computes the Z-scores of the latest data point based on historical data
    fn compute_z_scores(&self) -> Option<[f64; 4]> {
        let n = self.data.len();

        // Need at least two data points to compute standard deviation
        if n < 2 {
            return None;
        }

        let mut means = [0.0; 4];
        let mut variances = [0.0; 4];

        // Calculate means
        for data_point in &self.data {
            for i in 0..4 {
                means[i] += data_point[i];
            }
        }
        for mean in &mut means {
            *mean /= n as f64;
        }

        // Calculate variances
        for data_point in &self.data {
            for i in 0..4 {
                variances[i] += (data_point[i] - means[i]).powi(2);
            }
        }
        for variance in &mut variances {
            *variance /= (n - 1) as f64; // Sample variance
        }

        // Calculate standard deviations
        let stddevs: Vec<f64> = variances.iter().map(|&var| var.sqrt()).collect();

        // Compute Z-scores for the latest data point
        let latest_data = self.data.back().unwrap();
        let mut z_scores = [0.0; 4];
        for i in 0..4 {
            if stddevs[i] != 0.0 {
                z_scores[i] = (latest_data[i] - means[i]) / stddevs[i];
            } else {
                z_scores[i] = 0.0; // Avoid division by zero
            }
        }

        Some(z_scores)
    }
}

fn main() {
    // Initialize the logger
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    let mut history = MetricHistory::new();

    loop {
        // Collect system metrics
        let (cpu_usage, ram_usage, disk_usage, network_latency) = collect_metrics();

        // Add metrics to history
        history.add(cpu_usage, ram_usage, disk_usage, network_latency);

        // Analyze the latest metrics for anomalies
        let metrics_status = analyze_status(&history);

        // Log the metrics and their status
        log_status(&metrics_status);

        // Sleep until the next monitoring interval
        thread::sleep(MONITORING_INTERVAL);
    }
}

/// Collects the current system metrics
fn collect_metrics() -> (f64, f64, f64, f64) {
    let mut sys = System::new_all();
    sys.refresh_all();

    // CPU usage in percentage
    let cpu_usage = sys.global_cpu_usage() as f64;

    // RAM usage in percentage
    let ram_usage = sys.used_memory() as f64 / sys.total_memory() as f64 * 100.0;

    // Disk usage in percentage

    let disks = Disks::new_with_refreshed_list();

    let disk_usage = disks
        .iter()
        .map(|d| {
            let total = d.total_space() as f64;
            let available = d.available_space() as f64;
            if total > 0.0 {
                (total - available) / total * 100.0
            } else {
                0.0
            }
        })
        .sum::<f64>()
        / disks.len() as f64; // Average disk usage across disks

    // Network latency to a well-known website
    let network_latency = check_network_latency("https://www.google.com");

    (cpu_usage, ram_usage, disk_usage, network_latency)
}

/// Checks network latency by sending a request to the given URL
fn check_network_latency(url: &str) -> f64 {
    let start = Instant::now();
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();

    match client.get(url).send() {
        Ok(_) => start.elapsed().as_secs_f64() * 1000.0, // Convert to milliseconds
        Err(_) => f64::MAX,                              // Use a large value to indicate failure
    }
}

/// Analyzes the latest metrics for anomalies using Z-scores
fn analyze_status(history: &MetricHistory) -> Metrics {
    if let Some(z_scores) = history.compute_z_scores() {
        // Use the Z-score threshold to determine anomalies
        let latest_data = history.data.back().unwrap();

        // Map each metric to its status
        let statuses: Vec<MetricStatus> = z_scores
            .iter()
            .map(|&z_score| {
                if z_score.abs() > ANOMALY_THRESHOLD {
                    MetricStatus::Critical
                } else {
                    MetricStatus::Normal
                }
            })
            .collect();

        Metrics {
            cpu: Metric {
                value: latest_data[0],
                status: statuses[0].clone(),
            },
            ram: Metric {
                value: latest_data[1],
                status: statuses[1].clone(),
            },
            disk: Metric {
                value: latest_data[2],
                status: statuses[2].clone(),
            },
            network: Metric {
                value: latest_data[3],
                status: statuses[3].clone(),
            },
        }
    } else {
        // Not enough data to compute Z-scores, default to Normal
        let latest_data = history.data.back().unwrap();
        Metrics {
            cpu: Metric {
                value: latest_data[0],
                status: MetricStatus::Normal,
            },
            ram: Metric {
                value: latest_data[1],
                status: MetricStatus::Normal,
            },
            disk: Metric {
                value: latest_data[2],
                status: MetricStatus::Normal,
            },
            network: Metric {
                value: latest_data[3],
                status: MetricStatus::Normal,
            },
        }
    }
}

/// Logs the current metrics and their statuses
fn log_status(metrics: &Metrics) {
    let now = Utc::now();
    let json_output = serde_json::to_string(metrics).unwrap();
    println!("[{}] Metrics: {}", now, json_output);
    info!("[{}] Metrics: {}", now, json_output);
}
