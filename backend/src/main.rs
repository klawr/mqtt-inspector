use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::protocol::Message;

mod jsonrpc;
mod mqtt;

type MqttMap = mqtt::MqttMap;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, UnboundedSender<Message>>>>;

fn send_message_to_peers(peer_map: &PeerMap, source: &str, topic: &str, payload: &bytes::Bytes) {
    peer_map.lock().unwrap().iter().for_each(|(addr, tx)| {
        let message = serde_json::json!({
            "source": source,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "topic": topic,
            "payload": payload.to_vec(), // Convert Bytes to Vec<u8>
        });

        match tx.unbounded_send(Message::Text(message.to_string())) {
            Ok(_) => { /* Implement Logging */ }
            Err(err) => println!("Error sending message to {}: {:?}", addr, err),
        }
    });
}

fn loop_forever(mut connection: rumqttc::Connection, peer_map: &PeerMap) {
    let (ip, port) = connection.eventloop.mqtt_options.broker_address();
    let source = format!("{}:{}", ip, port);
    for (_i, notification) in connection.iter().enumerate() {
        match notification {
            Ok(rumqttc::Event::Incoming(rumqttc::Packet::Publish(p))) => {
                send_message_to_peers(peer_map, &source, &p.topic, &p.payload);
                // Handle other types of events if necessary
            }
            Ok(rumqttc::Event::Incoming(rumqttc::Packet::ConnAck(a))) => {
                println!("Connection event: {:?}", a);
            }
            Ok(_) => {
                // Handle other types of events if necessary
            }
            Err(err) => {
                // Handle connection error
                println!("Connection error: {:?}", err);
            }
        }
    }
}

fn connect_to_mqtt_client(mqtt_host: &SocketAddr, mqtt_map: MqttMap, peer_map: PeerMap) -> () {
    let ip = mqtt_host.ip().to_string();
    let port = mqtt_host.port();
    let mut mqtt_lock = mqtt_map.lock().unwrap();
    let mqtt_client = mqtt_lock
        .iter()
        .find(|client| client.0.ip().to_string() == ip && client.0.port() == port);

    if mqtt_client.is_some() {
        println!("MQTT-Client for {} already exists.", mqtt_host);
    } else {
        println!(
            "MQTT-Client for {} does not exist. Creating new client.",
            mqtt_host
        );
        let (client, connection) = mqtt::connect_to_mqtt_host(&ip, port);
        mqtt_lock.insert(*mqtt_host, client);
        drop(mqtt_lock);

        loop_forever(connection, &peer_map);
    }
}

fn connect_to_broker(mqtt_ip: &str, mqtt_port: &str, peer_map: PeerMap, mqtt_map: MqttMap) {
    let mqtt_host: SocketAddr = format!("{}:{}", mqtt_ip, mqtt_port)
        .parse()
        .expect("Failed to parse MQTT host and port");
    let mqtt_map_clone = mqtt_map.clone();
    let peer_map_clone = peer_map.clone();

    std::thread::spawn(move || {
        connect_to_mqtt_client(&mqtt_host, mqtt_map_clone, peer_map_clone);
    });
}

fn deserialize_json_rpc_and_process(json_rpc: &str, peer_map: PeerMap, mqtt_map: MqttMap) -> () {
    let result = jsonrpc::deserialize_json_rpc(json_rpc);
    if result.is_err() {
        println!("Error deserializing JSON-RPC: {:?}", result.err());
        return;
    }
    let message = result.unwrap();
    println!(
        "Got method \"{}\" with params {}",
        message.method,
        message.params.clone().unwrap()
    );
    match message.method.as_str() {
        "connect" => {
            let (ip, port) = jsonrpc::get_ip_and_port(message.params);
            connect_to_broker(&ip, &port, peer_map, mqtt_map);
        }
        "publish" => {
            let (ip, port, topic, payload) = jsonrpc::get_ip_port_topic_and_payload(message.params);
            mqtt::publish_message(&ip, &port, &topic, &payload, mqtt_map);
            // Implement publishing to a topic
        }
        _ => {
            // Implement other methods
        }
    }
    // Implement deserialization and processing of JSON-RPC message
}

async fn handle_connection(
    peer_map: PeerMap,
    raw_stream: TcpStream,
    client_addr: SocketAddr,
    mqtt_map: MqttMap,
) -> () {
    println!("Incoming TCP connection from: {}", client_addr);

    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");
    println!("WebSocket connection established: {}", client_addr);

    let (tx, rx) = unbounded();
    peer_map.lock().unwrap().insert(client_addr, tx);

    let (outgoing, incoming) = ws_stream.split();

    let broadcast_incoming = incoming.try_for_each(|msg| {
        deserialize_json_rpc_and_process(
            &msg.to_text().unwrap(),
            peer_map.clone(),
            mqtt_map.clone(),
        );

        future::ok(())
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    println!("{} disconnected", &client_addr);
    peer_map.lock().unwrap().remove(&client_addr);
}

#[tokio::main]
async fn main() -> () {
    let mqtt_map = MqttMap::new(Mutex::new(HashMap::new()));

    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 3030);
    let ws_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080);
    let peer_map = PeerMap::new(Mutex::new(HashMap::new()));
    let try_socket = TcpListener::bind(&ws_addr).await;
    let listener = try_socket.expect("Failed to bind");

    println!("Listening for browser connections on {}", server_addr);
    tokio::spawn(async move {
        warp::serve(warp::fs::dir("../frontend/wwwroot"))
            .run(server_addr)
            .await;
    });

    println!("Listening for websocket connections on {}", ws_addr);
    while let Ok((stream, client_addr)) = listener.accept().await {
        tokio::spawn(handle_connection(
            peer_map.clone(),
            stream,
            client_addr,
            mqtt_map.clone(),
        ));
    }
}
