use serde::Serialize;

#[derive(Debug, Serialize)]
pub enum MetricStatus {
    Normal,
    SlightlyAbnormal,
    Abnormal,
    Critical,
}

#[derive(Debug, Serialize)]
pub struct Metrics {
    pub cpu: Metric,
    pub ram: Metric,
    pub disk: Metric,
    pub network: Metric,
}

#[derive(Debug, Serialize)]
pub struct Metric {
    pub value: f64,
    pub status: MetricStatus,
}