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
) -> Response {
    let roomid = RoomId(roomid);
    let (sender, receiver) = tokio::sync::mpsc::channel(10);
    if state.add_room_message_receiver(&roomid, sender) {
        Sse::new(ReceiverStream::new(receiver).map(|data| Event::default().json_data(data)))
            .into_response()
    } else {
        Json(map_1("msg", "no_room".to_string())).into_response()
    }
}

async fn post_room_message(
    State(state): State<Arc<AppState>>,
    Path(roomid): Path<String>,
    Json(msg): Json<RoomMessage>,
) -> impl IntoResponse {
    let roomid = RoomId(roomid);
    if state.rooms.contains_key(&roomid) {
        state.send_message(&roomid, msg);
        Json(map_1("msg", "room_exists,post_room_message".to_string()))
    } else {
        state.add_room(&roomid);
        state.send_message(&roomid, msg);
        Json(map_1("msg", "new_room,post_room_message".to_string()))
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct RoomMessage {
    msg: String,
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

    fn add_room(&self, id: &RoomId) {
        tracing::info!("add_room {id:?}");
        self.rooms.insert(
            id.clone(),
            Room {
                id: id.clone(),
                clients: DashMap::new(),
                message_receivers: Vec::new(),
            },
        );
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

    fn send_message(&self, id: &RoomId, msg: RoomMessage) {
        if let Some(mut room) = self.rooms.get_mut(id) {
            let msg = Arc::new(msg);
            tracing::info!(
                "send_message {id:?} receivers={}",
                room.message_receivers.len()
            );
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
