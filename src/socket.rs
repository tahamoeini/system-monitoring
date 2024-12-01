use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{timeout, Duration};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message, MaybeTlsStream};

#[derive(Debug, Serialize, Deserialize)]
pub struct WSMessage {
    pub sub: String,
    pub payload: Option<String>,
    pub reply_sub: Option<String>,
    pub error: Option<String>,
}

pub struct WebSocketClient {
    ws_stream: Arc<Mutex<tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>>>,
    sender: mpsc::Sender<WSMessage>,
    pending_replies: Arc<Mutex<HashMap<String, mpsc::Sender<WSMessage>>>>,
}

impl WebSocketClient {
    pub async fn new(url: &str) -> Result<Self, String> {
        let (ws_stream, _) = connect_async(url).await.map_err(|e| format!("Failed to connect: {}", e))?;
        info!("Connected to WebSocket server at {}", url);

        let (tx, mut rx) = mpsc::channel::<WSMessage>(32);
        let pending_replies: Arc<Mutex<HashMap<String, mpsc::Sender<WSMessage>>>> = Arc::new(Mutex::new(HashMap::new()));
        let ws_stream = Arc::new(Mutex::new(ws_stream));

        let ws_stream_clone = Arc::clone(&ws_stream);
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                let json_msg = serde_json::to_string(&msg).unwrap();
                if let Err(e) = ws_stream_clone.lock().await.send(Message::Text(json_msg)).await {
                    error!("Error sending message: {}", e);
                }
            }
        });

        let pending_replies_clone = Arc::clone(&pending_replies);
        let ws_stream_clone = Arc::clone(&ws_stream);
        tokio::spawn(async move {
            while let Some(msg) = ws_stream_clone.lock().await.next().await {
                if let Ok(Message::Text(text)) = msg {
                    match serde_json::from_str::<WSMessage>(&text) {
                        Ok(ws_msg) => {
                            info!("Received: {:?}", ws_msg);
                            let mut pending = pending_replies_clone.lock().await;
                            if let Some(sender) = pending.remove(&ws_msg.sub) {
                                let _ = sender.send(ws_msg).await;
                            }
                        }
                        Err(e) => error!("Failed to deserialize message: {}", e),
                    }
                } else if let Ok(Message::Close(_)) = msg {
                    info!("Connection closed by server");
                    break;
                } else if let Err(e) = msg {
                    error!("WebSocket error: {}", e);
                    break;
                }
            }
        });

        Ok(Self {
            ws_stream,
            sender: tx,
            pending_replies,
        })
    }

    pub async fn send_and_wait_for_reply(&self, msg: WSMessage) -> Result<WSMessage, String> {
        if let Some(reply_sub) = &msg.reply_sub {
            let (reply_tx, mut reply_rx) = mpsc::channel(1);
            self.pending_replies.lock().await.insert(reply_sub.clone(), reply_tx);

            self.sender.send(msg).await.map_err(|e| format!("Failed to send message: {}", e))?;

            match timeout(Duration::from_secs(60), reply_rx.recv()).await {
                Ok(Some(reply)) => Ok(reply),
                Ok(None) => Err("Reply channel closed unexpectedly".to_string()),
                Err(_) => Err("Timeout waiting for reply".to_string()),
            }
        } else {
            self.sender.send(msg).await.map_err(|e| format!("Failed to send message: {}", e))?;
            Ok(WSMessage {
                sub: "NoReplyExpected".to_string(),
                payload: None,
                reply_sub: None,
                error: None,
            })
        }
    }
}
