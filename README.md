# shuttle-ramp P2P file sharing via WebRTC

This server provides rooms where clients can meet to share files via a direct client to client connection.

The first client posts peer-to-peer WebRTC data channel connection information to the room.
The server forwards this information to the other client eventually.
The other client posts its connection answer and it is forwarded to the first client.
The connection information and answer message format is some reduced JSON format,
which is used to reconstruct the WebRTC SDP signalling.
By restricting this JSON format the server can not be used to transmit arbitrary messages to each others WebRTC engine directly.

Once a data channel is established, the clients can exchange messages to transmit files.
Clients should make sure that large files get written directly to filesystem, without large buffers
in the browser JS runtime itself.