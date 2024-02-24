use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcMessage {
    jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
    id: String,
}

#[derive(Debug)]
pub enum JsonRpcError {
    DeserializationError(serde_json::Error),
}

pub fn deserialize_json_rpc(json_rpc: &str) -> Result<JsonRpcMessage, JsonRpcError> {
    match serde_json::from_str(json_rpc) {
        Ok(result) => Ok(result),
        Err(error) => Err(JsonRpcError::DeserializationError(error)),
    }
}

pub fn get_ip_and_port(params: Option<serde_json::Value>) -> (String, String) {
    let params = params.unwrap();
    let ip = params["ip"].as_str().unwrap();
    let port = params["port"].as_str().unwrap();

    (ip.to_string(), port.to_string())
}

pub fn get_ip_port_topic_and_payload(
    params: Option<serde_json::Value>,
) -> (String, String, String, String) {
    let params = params.unwrap();
    let ip = params["ip"].as_str().unwrap();
    let port = params["port"].as_str().unwrap();
    let topic = params["topic"].as_str().unwrap();
    let payload = params["payload"].as_str().unwrap();

    (
        ip.to_string(),
        port.to_string(),
        topic.to_string(),
        payload.to_string(),
    )
}
