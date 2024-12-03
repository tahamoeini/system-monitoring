use communication::{communication_client::CommunicationClient, Acknowledge, Event};
use tonic::{transport::Server, Request, Response, Status};

pub mod communication {
    tonic::include_proto!("communication");
}

#[tokio::main]
async fn main(subject: String, payload: String, reply: Option<bool>) -> Result<Acknowledge, Box<dyn std::error::Error>> {
    let channel = tonic::transport::Channel::from_static("http://[::1]:50051")
        .connect()
        .await?;
    let mut client = communication::communication_client::CommunicationClient::new(channel);
    let request = Request::new(Event {
        subject,
        payload,
        reply: reply.unwrap_or(false),
    });
    let response: Acknowledge = client.alert(request).await?.into_inner();
    if response.status == ICUStatus::Failure as i32 {
        error!("Failed to send alert: {}", response.payload);
    } else {
        info!("Sent alert: {}", response.payload);
    }
    Ok(response)
}