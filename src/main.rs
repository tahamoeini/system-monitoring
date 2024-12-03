mod metrics;
mod server;
mod client;
mod types;

use crate::metrics::{Metric, MetricHistory, MetricStatus, Metrics};
use chrono::Utc;
use client::send_alert;
use env_logger;
use extended_isolation_forest::{Forest, ForestOptions};
use log::{error, info};
use reqwest;
use server::start_server;
use std::{thread, time::{Duration, Instant}};
use sysinfo::{Disks, System};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

const MONITORING_INTERVAL: Duration = Duration::from_secs(5);

fn main() {
    env_logger::init();

    let (tx, mut rx) = mpsc::channel(1);

    thread::spawn(move || {
        let rt = Runtime::new().unwrap();
        rt.block_on(start_server(tx)).unwrap();
    });

    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        rx.recv().await.unwrap();
    });

    info!("Server is running. Starting monitoring...");

    let mut history = MetricHistory::new();

    loop {
        let (cpu_usage, ram_usage, disk_usage, network_latency) = collect_metrics();

        history.add(cpu_usage, ram_usage, disk_usage, network_latency);

        if history.data.len() >= 20 {
            let metrics_status = analyze_status(&mut history);
            log_status(&metrics_status);

            if metrics_status.overall_status == MetricStatus::Critical {
                let payload = format!(
                    "Critical: CPU {:.2}%, RAM {:.2}%, Disk {:.2}%, Network {:.2}ms",
                    metrics_status.cpu.value,
                    metrics_status.ram.value,
                    metrics_status.disk.value,
                    metrics_status.network.value
                );

                let result = rt.block_on(send_alert("CriticalAlert".to_string(), payload, true));
                match result {
                    Ok(ack) => info!("Alert sent: {:?}", ack),
                    Err(e) => error!("Alert failed: {}", e),
                }
            }
        } else {
            println!("Collecting data... ({}/{})", history.data.len(), 20);
        }

        thread::sleep(MONITORING_INTERVAL);
    }
}

fn collect_metrics() -> (f64, f64, f64, f64) {
    let mut sys = System::new_all();
    sys.refresh_all();

    let cpu_usage = sys.global_cpu_usage() as f64;

    let ram_usage = sys.used_memory() as f64 / sys.total_memory() as f64 * 100.0;

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
        / disks.len() as f64;

    let network_latency = check_network_latency("https://www.google.com");

    (cpu_usage, ram_usage, disk_usage, network_latency)
}

fn check_network_latency(url: &str) -> f64 {
    let start = Instant::now();
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();

    match client.get(url).send() {
        Ok(_) => start.elapsed().as_secs_f64() * 1000.0,
        Err(_) => f64::MAX,
    }
}

fn analyze_status(history: &mut MetricHistory) -> Metrics {
    let data_len = history.data.len();

    let training_data: Vec<[f64; 4]> = history.data.iter().cloned().collect();

    let options = ForestOptions {
        n_trees: 100,
        sample_size: data_len,
        max_tree_depth: None,
        extension_level: 1,
    };

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

    let latest_data = *history.data.back().unwrap();

    let latest_scores = [
        forest.score(&[latest_data[0], 0.0, 0.0, 0.0]),
        forest.score(&[0.0, latest_data[1], 0.0, 0.0]),
        forest.score(&[0.0, 0.0, latest_data[2], 0.0]),
        forest.score(&[0.0, 0.0, 0.0, latest_data[3]]),
    ];

    history.update_smoothed_scores(latest_scores);

    let spikes = history.detect_spike(latest_data);

    let coefficients = [0.4, 0.35, 0.15, 0.1];

    let thresholds = [0.7, 0.6, 0.65, 0.75];

    let statuses: Vec<MetricStatus> = history
        .smoothed_scores
        .iter()
        .zip(&thresholds)
        .enumerate()
        .map(|(i, (&score, &threshold))| {
            if spikes[i] {
                MetricStatus::Warning
            } else {
                determine_status(score, threshold)
            }
        })
        .collect();

    let overall_score: f64 = history
        .smoothed_scores
        .iter()
        .zip(&coefficients)
        .map(|(&score, &coeff)| score * coeff)
        .sum();

    let overall_status = if overall_score > 0.8 {
        MetricStatus::Critical
    } else if overall_score > 0.6 {
        MetricStatus::Warning
    } else {
        MetricStatus::Normal
    };

    info!(
        "Scores - CPU: {:.2}, RAM: {:.2}, Disk: {:.2}, Network: {:.2}, Overall: {:.2}",
        history.smoothed_scores[0],
        history.smoothed_scores[1],
        history.smoothed_scores[2],
        history.smoothed_scores[3],
        overall_score
    );

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

fn determine_status(score: f64, threshold: f64) -> MetricStatus {
    if score > threshold + 0.2 {
        MetricStatus::Critical
    } else if score > threshold {
        MetricStatus::Warning
    } else {
        MetricStatus::Normal
    }
}

fn log_status(metrics: &Metrics) {
    let now = Utc::now();
    info!(
        "[{}] Metrics - CPU: {:.2}%, RAM: {:.2}%, Disk: {:.2}%, Network: {:.2}ms, Status: {:?}",
        now,
        metrics.cpu.value,
        metrics.ram.value,
        metrics.disk.value,
        metrics.network.value,
        metrics.overall_status
    );
}
