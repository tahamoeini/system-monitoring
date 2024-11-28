use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
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
