use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, State},
    response::{self, sse::Event, Html, IntoResponse, Response, Sse},
    routing::{get, post},
    Json, Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{error::TrySendError, Sender};
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tower_http::trace::TraceLayer;

#[shuttle_runtime::main]
async fn main() -> shuttle_axum::ShuttleAxum {
    tracing::info!("hello");

    let router = Router::new()
        .route("/", get(get_index_html))
        .route("/:roomid", get(get_room_messages))
        .route("/:roomid", post(post_room_message))
        .layer(TraceLayer::new_for_http())
        .with_state(Arc::new(AppState::new()));

    Ok(router.into())
}

async fn get_index_html() -> response::Result<impl IntoResponse> {
    let content = tokio::fs::read_to_string("src/index.html").await.unwrap();
    Ok(Html(content))
}

async fn get_room_messages(
    State(state): State<Arc<AppState>>,
    Path(roomid): Path<String>,
) -> impl IntoResponse {
    let roomid = RoomId(roomid);
    let (sender, receiver) = tokio::sync::mpsc::channel(10);
    state.ensure_room(&roomid);
    state.add_room_message_receiver(&roomid, sender);
    Sse::new(ReceiverStream::new(receiver).map(|data| Event::default().json_data(data)))
}

async fn post_room_message(
    State(state): State<Arc<AppState>>,
    Path(roomid): Path<String>,
    Json(msg): Json<RoomMessage>,
) -> impl IntoResponse {
    let roomid = RoomId(roomid);
    state.ensure_room(&roomid);
    state.send_message(&roomid, msg);
    Json(map_1("msg", "post_room_message".to_string()))
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(tag="msg", rename_all="snake_case")]
enum RoomMessage {
    OfferFile { file: u32 },
    AcceptFileOfferConnection { file: u32, offer_sdp: String },
    AnswerConnection { answer_sdp: String },
}

struct AppState {
    rooms: DashMap<RoomId, Room>,
}

impl AppState {
    fn new() -> Self {
        Self {
            rooms: DashMap::default(),
        }
    }

    fn ensure_room(&self, id: &RoomId) -> bool {
        if self.rooms.contains_key(id) {
            true
        } else {
            tracing::info!("ensure_room {id:?}");
            self.rooms.insert(
                id.clone(),
                Room {
                    id: id.clone(),
                    clients: DashMap::new(),
                    message_receivers: Vec::new(),
                },
            );
            false
        }
    }

    fn add_room_message_receiver(&self, id: &RoomId, sender: Sender<Arc<RoomMessage>>) -> bool {
        if let Some(mut room) = self.rooms.get_mut(id) {
            tracing::info!("add_room_message_receiver {id:?}");
            room.message_receivers.push(sender);
            true
        } else {
            tracing::warn!("add_room_message_receiver, no room {id:?}");
            false
        }
    }

    fn send_message(&self, id: &RoomId, msg: RoomMessage) -> bool {
        if let Some(mut room) = self.rooms.get_mut(id) {
            tracing::info!(
                "send_message {id:?} receivers={}",
                room.message_receivers.len()
            );
            let msg = Arc::new(msg);
            room.message_receivers.retain(|receiver| {
                match receiver.try_send(msg.clone()) {
                    Ok(_) => true,
                    Err(TrySendError::Full(_)) => {
                        tracing::warn!("full"); // could spawn now a task
                        true
                    }
                    Err(TrySendError::Closed(_)) => false,
                }
            });
            true
        } else {
            tracing::warn!("send_message, no room {id:?}");
            false
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
struct RoomId(String);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
struct ClientId(String);

struct Room {
    id: RoomId,
    clients: DashMap<ClientId, Client>,
    message_receivers: Vec<Sender<Arc<RoomMessage>>>,
}

struct Client {
    id: ClientId,
    msg_to_client: (),
}

fn map_1<V>(k: impl Into<String>, v: V) -> HashMap<String, V> {
    HashMap::from([(k.into(), v)])
}
