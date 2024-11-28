use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) enum MetricStatus {
    Normal,
    SlightlyAbnormal,
    Abnormal,
    Critical,
}

#[derive(Debug, Serialize)]
pub(crate) struct Metrics {
    pub cpu: Metric,
    pub ram: Metric,
    pub disk: Metric,
    pub network: Metric,
}

#[derive(Debug, Serialize)]
pub(crate) struct Metric {
    pub value: f64,
    pub status: MetricStatus,
}
