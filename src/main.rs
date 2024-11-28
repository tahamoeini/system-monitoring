mod metrics;

use crate::metrics::{Metric, MetricStatus, Metrics};
use chrono::Utc;
use env_logger;
use extended_isolation_forest::{Forest, ForestOptions};
use log::info;
use reqwest;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use std::{env, thread};
use sysinfo::{Disks, System};

const MAX_HISTORY: usize = 100;
const MONITORING_INTERVAL: Duration = Duration::from_secs(1);
const SMOOTHING_ALPHA: f64 = 0.3; // Smoothing factor for EWMA
const SPIKE_THRESHOLD: f64 = 20.0; // Threshold for detecting rapid spikes

#[derive(Debug, Clone)]
struct MetricHistory {
    data: VecDeque<[f64; 4]>, // Each data point contains [CPU, RAM, Disk, Network Latency]
    smoothed_scores: [f64; 4], // Smoothed scores for [CPU, RAM, Disk, Network]
    previous_data: Option<[f64; 4]>, // Previous metrics for spike detection
}

impl MetricHistory {
    fn new() -> Self {
        Self {
            data: VecDeque::with_capacity(MAX_HISTORY),
            smoothed_scores: [0.0; 4],
            previous_data: None,
        }
    }

    fn add(&mut self, cpu: f64, ram: f64, disk: f64, latency: f64) {
        // Maintain the history size
        if self.data.len() == MAX_HISTORY {
            self.data.pop_front();
        }
        self.previous_data = self.data.back().cloned(); // Store previous data for spike detection
        self.data.push_back([cpu, ram, disk, latency]);
    }

    /// Update smoothed scores using EWMA
    fn update_smoothed_scores(&mut self, latest_scores: [f64; 4]) {
        for i in 0..4 {
            self.smoothed_scores[i] = SMOOTHING_ALPHA * latest_scores[i]
                + (1.0 - SMOOTHING_ALPHA) * self.smoothed_scores[i];
        }
    }

    /// Detect rapid spikes based on the difference between consecutive data points
    fn detect_spike(&self, current_data: [f64; 4]) -> [bool; 4] {
        if let Some(prev) = self.previous_data {
            let mut spikes = [false; 4];
            for i in 0..4 {
                if (current_data[i] - prev[i]).abs() > SPIKE_THRESHOLD {
                    spikes[i] = true;
                }
            }
            spikes
        } else {
            [false; 4] // No spike detected if there is no previous data
        }
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
            let metrics_status = analyze_status(&mut history);
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

/// Analyzes the latest metrics for anomalies using EWMA, spike detection, and thresholds
fn analyze_status(history: &mut MetricHistory) -> Metrics {
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
                overall_status: MetricStatus::Normal,
            };
        }
    };

    // Get the latest data point
    let latest_data = *history.data.back().unwrap();

    // Compute anomaly scores for each metric
    let latest_scores = [
        forest.score(&[latest_data[0], 0.0, 0.0, 0.0]),
        forest.score(&[0.0, latest_data[1], 0.0, 0.0]),
        forest.score(&[0.0, 0.0, latest_data[2], 0.0]),
        forest.score(&[0.0, 0.0, 0.0, latest_data[3]]),
    ];

    // Update smoothed scores
    history.update_smoothed_scores(latest_scores);

    // Detect spikes
    let spikes = history.detect_spike(latest_data);

    // Assign coefficients to each metric
    let coefficients = [0.4, 0.35, 0.15, 0.1];

    // Individual metric thresholds
    let thresholds = [0.7, 0.6, 0.65, 0.75];

    // Determine individual metric statuses, incorporating spike detection
    let statuses: Vec<MetricStatus> = history
        .smoothed_scores
        .iter()
        .zip(&thresholds)
        .enumerate()
        .map(|(i, (&score, &threshold))| {
            if spikes[i] {
                MetricStatus::Warning // Escalate to Warning if a spike is detected
            } else {
                determine_status(score, threshold)
            }
        })
        .collect();

    // Compute overall weighted anomaly score
    let overall_score: f64 = history
        .smoothed_scores
        .iter()
        .zip(&coefficients)
        .map(|(&score, &coeff)| score * coeff)
        .sum();

    // Determine overall status
    let overall_status = if overall_score > 0.8 {
        MetricStatus::Critical
    } else if overall_score > 0.6 {
        MetricStatus::Warning
    } else {
        MetricStatus::Normal
    };

    // Log raw scores for debugging
    info!(
        "Scores - CPU: {:.2}, RAM: {:.2}, Disk: {:.2}, Network: {:.2}, Overall: {:.2}",
        history.smoothed_scores[0],
        history.smoothed_scores[1],
        history.smoothed_scores[2],
        history.smoothed_scores[3],
        overall_score
    );

    // Return the Metrics struct with individual and overall statuses
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
        overall_status,
    }
}

/// Determines the status of a metric based on its smoothed anomaly score and threshold
fn determine_status(score: f64, threshold: f64) -> MetricStatus {
    if score > threshold + 0.2 {
        // Critical threshold
        MetricStatus::Critical
    } else if score > threshold {
        // Warning threshold
        MetricStatus::Warning
    } else {
        MetricStatus::Normal
    }
}

fn log_status(metrics: &Metrics) {
    let now = Utc::now();
    let json_output = serde_json::to_string(metrics).unwrap();
    info!("[{}] Metrics: {}", now, json_output);
}
