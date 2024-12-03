pub struct Event {
    pub subject: String,
    pub payload: String,
    reply: bool,
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
        if (subject.split('-').collect()[0] == "ack") {
            Self {
                subject,
                payload,
                status,
            }
        } else {
            Self {
                subject: format!("ack-{}", subject),
                payload: if (error.is_some()) { error.unwrap().message } else { payload },
                status: if (error.is_some()) { ICUStatus::Failure } else { ICUStatus::Success },
            }
        }
    }
}

pub enum ICUStatus {
    Failure = 0,
    Success,
}

pub enum Status {
    Ok = 0,
    Cancelled,
    Unknown,
    InvalidArgument,
    DeadlineExceeded,
    NotFound,
    AlreadyExists,
    PermissionDenied,
    ResourceExhausted,
    FailedPrecondition,
    Aborted,
    OutOfRange,
    Unimplemented,
    Internal,
    Unavailable,
    DataLoss,
    Unauthenticated,
}

pub struct ICUError {
    pub code: Status,
    pub message: String,
}