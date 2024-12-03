use tonic::Status;

pub struct Event {
    pub subject: String,
    pub payload: String,
    pub reply: bool,
}

impl Event {
    pub fn new(subject: String, payload: String, reply: bool) -> Self {
        Self {
            subject,
            payload,
            reply,
        }
    }
}

pub struct EventAck {
    pub subject: String,
    pub payload: String,
    pub status: ICUStatus,
}

impl EventAck {
    pub fn new(subject: String, payload: String, status: ICUStatus, error: Option<ICUError>) -> Self {
        if subject.starts_with("ack-") {
            Self { subject, payload, status }
        } else {
            Self {
                subject: format!("ack-{}", subject),
                payload: error
                    .as_ref()
                    .map_or(payload, |e| e.message.clone()),
                status: error.map_or(ICUStatus::Success, |_| ICUStatus::Failure),
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ICUStatus {
    Failure = 0,
    Success = 1,
}

pub struct ICUError {
    pub code: Status,
    pub message: String,
}
