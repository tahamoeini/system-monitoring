use crate::types::ICUStatus;
use communication::events_server::{Events, EventsServer};
use communication::{Acknowledge, Event};
use tokio::sync::mpsc;
use tonic::{transport::Server, Request, Response, Status};

pub mod communication {
    tonic::include_proto!("communication");
}

#[derive(Debug, Default)]
pub struct EventsService {}

#[tonic::async_trait]
impl Events for EventsService {
    async fn alert(&self, request: Request<Event>) -> Result<Response<Acknowledge>, Status> {
        let event = request.into_inner();
        if event.reply {
            Ok(Response::new(Acknowledge {
                subject: format!("ack-{}", event.subject),
                status: ICUStatus::Success as i32,
                payload: "Acknowledged successfully.".to_string(),
            }))
        } else {
            Ok(Response::new(Acknowledge {
                subject: event.subject,
                status: ICUStatus::Failure as i32,
                payload: "Acknowledgement failed.".to_string(),
            }))
        }
    }
}

pub async fn start_server(tx: mpsc::Sender<()>) -> Result<(), Box<dyn std::error::Error>> {
    let address = "[::1]:50051".parse()?;
    let service = EventsService::default();

    // Notify the main thread that the server is ready
    tx.send(()).await.unwrap();

    println!("Server listening on {}", address);

    Server::builder()
        .add_service(EventsServer::new(service)) // Updated to use EventsServer
        .serve(address)
        .await?;

    Ok(())
}
