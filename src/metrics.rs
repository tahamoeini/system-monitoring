use serde::Serialize;
use std::collections::VecDeque;

pub const MAX_HISTORY: usize = 100;
pub const SPIKE_THRESHOLD: f64 = 20.0; // Threshold for detecting rapid spikes
pub const SMOOTHING_ALPHA: f64 = 0.3; // Smoothing factor for EWMA

#[derive(Debug, Clone)]
pub struct MetricHistory {
    pub data: VecDeque<[f64; 4]>, // Each data point contains [CPU, RAM, Disk, Network Latency]
    pub smoothed_scores: [f64; 4], // Smoothed scores for [CPU, RAM, Disk, Network]
    pub previous_data: Option<[f64; 4]>, // Previous metrics for spike detection
}

impl MetricHistory {
    pub fn new() -> Self {
        Self {
            data: VecDeque::with_capacity(MAX_HISTORY),
            smoothed_scores: [0.0; 4],
            previous_data: None,
        }
    }

    pub fn add(&mut self, cpu: f64, ram: f64, disk: f64, latency: f64) {
        // Maintain the history size
        if self.data.len() == MAX_HISTORY {
            self.data.pop_front();
        }
        self.previous_data = self.data.back().cloned(); // Store previous data for spike detection
        self.data.push_back([cpu, ram, disk, latency]);
    }

    /// Update smoothed scores using EWMA
    pub fn update_smoothed_scores(&mut self, latest_scores: [f64; 4]) {
        for i in 0..4 {
            self.smoothed_scores[i] = SMOOTHING_ALPHA * latest_scores[i]
                + (1.0 - SMOOTHING_ALPHA) * self.smoothed_scores[i];
        }
    }

    /// Detect rapid spikes based on the difference between consecutive data points
    pub fn detect_spike(&self, current_data: [f64; 4]) -> [bool; 4] {
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

#[derive(Debug, Serialize, Clone, PartialEq)]
pub enum MetricStatus {
    Normal,
    Warning,
    Critical,
}

#[derive(Debug, Serialize)]
pub struct Metrics {
    pub cpu: Metric,
    pub ram: Metric,
    pub disk: Metric,
    pub network: Metric,
    pub overall_status: MetricStatus,
}

#[derive(Debug, Serialize)]
pub struct Metric {
    pub value: f64,
    pub status: MetricStatus,
}
