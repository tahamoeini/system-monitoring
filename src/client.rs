use crate::types::ICUStatus;
use communication::events_client::EventsClient;
use communication::{Acknowledge, Event};
use log::{error, info};
use tonic::Request;

pub mod communication {
    tonic::include_proto!("communication");
}

pub(crate) async fn send_alert(
    subject: String,
    payload: String,
    reply: bool,
) -> Result<Acknowledge, Box<dyn std::error::Error>> {
    let channel = tonic::transport::Channel::from_static("http://[::1]:50051")
        .connect()
        .await?;
    let mut client = EventsClient::new(channel); // Updated to use EventsClient
    let request = Request::new(Event {
        subject,
        payload,
        reply,
    });

    let response: Acknowledge = client.alert(request).await?.into_inner();

    if response.status == ICUStatus::Failure as i32 {
        error!("Failed to send alert: {}", response.payload);
    } else {
        info!("Sent alert: {}", response.payload);
    }
    Ok(response)
}
