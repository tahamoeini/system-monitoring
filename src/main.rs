mod types;

use crate::types::{Metric, MetricStatus, Metrics};
use chrono::Utc;
use env_logger;
use extended_isolation_forest::{Forest, ForestOptions};
use log::info;
use ndarray::Array2;
use reqwest;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use std::{env, thread};
use sysinfo::{Disks, System};

const MAX_HISTORY: usize = 100;
const MONITORING_INTERVAL: Duration = Duration::from_secs(1);

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

        if history.data.len() >= 20 {
            // Analyze the latest metrics for anomalies
            let metrics_status = analyze_status(&history);
            // Log the metrics and their status
            log_status(&metrics_status);
        } else {
            println!("Collecting data... ({}/{})", history.data.len(), 20);
        }

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
    let data_len = history.data.len();

    // Convert history data into a slice of fixed-size arrays
    let training_data: Vec<[f64; 4]> = history.data.iter().cloned().collect();

    let options = ForestOptions {
        n_trees: 100,
        sample_size: data_len, // Use all available data for training
        max_tree_depth: None,
        extension_level: 1,
    };

    // Attempt to create the isolation forest
    let forest = match Forest::from_slice(&training_data, &options) {
        Ok(forest) => forest,
        Err(e) => {
            println!("Failed to train Isolation Forest: {}", e);
            return Metrics {
                cpu: Metric {
                    value: 0.0,
                    status: MetricStatus::Normal,
                },
                ram: Metric {
                    value: 0.0,
                    status: MetricStatus::Normal,
                },
                disk: Metric {
                    value: 0.0,
                    status: MetricStatus::Normal,
                },
                network: Metric {
                    value: 0.0,
                    status: MetricStatus::Normal,
                },
            };
        }
    };

    // Get the latest data point
    let latest_data = history.data.back().unwrap();

    // Compute the anomaly score for the latest data
    let latest_score = forest.score(latest_data);

    // Determine the metric status based on the anomaly score
    let status = if latest_score > 0.5 {
        MetricStatus::Critical
    } else {
        MetricStatus::Normal
    };

    // Create and return the Metrics struct
    Metrics {
        cpu: Metric {
            value: latest_data[0],
            status: status.clone(),
        },
        ram: Metric {
            value: latest_data[1],
            status: status.clone(),
        },
        disk: Metric {
            value: latest_data[2],
            status: status.clone(),
        },
        network: Metric {
            value: latest_data[3],
            status: status.clone(),
        },
    }
}

fn log_status(metrics: &Metrics) {
    let now = Utc::now();
    let json_output = serde_json::to_string(metrics).unwrap();
    println!("[{}] Metrics: {}", now, json_output);
    info!("[{}] Metrics: {}", now, json_output);
}
