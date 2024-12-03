mod metrics;

use crate::metrics::{Metric, MetricHistory, MetricStatus, Metrics};
use chrono::Utc;
use env_logger;
use extended_isolation_forest::{Forest, ForestOptions};
use log::{error, info};
use reqwest;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{env, thread};
use sysinfo::{Disks, System};
use tokio::runtime::Runtime;

const MONITORING_INTERVAL: Duration = Duration::from_secs(1);

fn main() {
    // Initialize the logger
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    let mut history = MetricHistory::new();
    let runtime = Runtime::new().unwrap();
    let client = runtime.block_on(async {});

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

            // Send message to server if critical
            if metrics_status.overall_status == MetricStatus::Critical {
                runtime.block_on(async {
                    // let message = WSMessage {
                    //     sub: "CriticalAlert".to_string(),
                    //     payload: Some(format!(
                    //         "Critical status detected: CPU: {:.2}, RAM: {:.2}, Disk: {:.2}, Network: {:.2}",
                    //         metrics_status.cpu.value,
                    //         metrics_status.ram.value,
                    //         metrics_status.disk.value,
                    //         metrics_status.network.value
                    //     )),
                    //     reply_sub: None,
                    //     error: None,
                    // };

                    // if let Err(e) = client.send_and_wait_for_reply(message).await {
                    //     error!("Failed to send critical alert: {}", e);
                    // }
                });
            }
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

/// Log the status
fn log_status(metrics: &Metrics) {
    let now = Utc::now();
    let json_output = serde_json::to_string(metrics).unwrap();
    info!("[{}] Metrics: {}", now, json_output);
}
