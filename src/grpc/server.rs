use communication::{communication_server::{Communication, CommunicationServer}, Acknowledge, Event};
use tonic::{transport::Server, Request, Response, Status};

pub mod communication {
    tonic::include_proto!("communication");
}

#[derive(Debug, Default)]
pub struct CommunicationService {}

#[tonic::async_trait]
impl Communication for CommunicationService {
    async fn alert(&self, request: Request<Event>) -> Result<Response<Acknowledge>, Status> {
        let event = request.into_inner();
        if event.reply {
            Ok(Response::new(Acknowledge {
                subject: format!("ack-{}", event.subject),
                status: ICUStatus::Success as i32,
                payload: "Acknowledged successfully.".to_string(),
            }))
            // } else {
            //     Ok(Response::new(Acknowledge {
            //         subject: event.subject,
            //         status: ICUStatus::Failure as i32,
            //         payload: "Acknowledgement failed.".to_string(),
            //     }))
        }
    }
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address = "[::1]:9000".parse().unwrap();
    let comm_service = CommunicationService::default();
    println!("Server listening on {}", addr);

    Server::builder().add_service(CommunicationServer::new(comm_service))
        .serve(address)
        .await?;
    Ok(())
}