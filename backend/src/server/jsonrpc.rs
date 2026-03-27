/*
 * Copyright (c) 2024-2026 Kai Lawrence
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcNotification<'a> {
    pub jsonrpc: &'a str,
    pub method: &'a str,
    pub params: serde_json::Value,
}

#[derive(Debug)]
pub enum JsonRpcError {
    DeserializationError,
}

pub fn deserialize_json_rpc(json_rpc: &str) -> Result<JsonRpcNotification<'_>, JsonRpcError> {
    match serde_json::from_str(json_rpc) {
        Ok(result) => Ok(result),
        Err(_) => Err(JsonRpcError::DeserializationError),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_valid_json_rpc() {
        let json = r#"{"jsonrpc":"2.0","method":"connect","params":{"hostname":"localhost:1883"}}"#;
        let result = deserialize_json_rpc(json);
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert_eq!(msg.jsonrpc, "2.0");
        assert_eq!(msg.method, "connect");
        assert_eq!(msg.params["hostname"], "localhost:1883");
    }

    #[test]
    fn test_deserialize_publish_message() {
        let json = r#"{"jsonrpc":"2.0","method":"publish","params":{"host":"broker:1883","topic":"test/topic","payload":"hello"}}"#;
        let result = deserialize_json_rpc(json);
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert_eq!(msg.method, "publish");
        assert_eq!(msg.params["host"], "broker:1883");
        assert_eq!(msg.params["topic"], "test/topic");
        assert_eq!(msg.params["payload"], "hello");
    }

    #[test]
    fn test_deserialize_save_command() {
        let json = r#"{"jsonrpc":"2.0","method":"save_command","params":{"name":"cmd1","topic":"t","payload":"p"}}"#;
        let result = deserialize_json_rpc(json);
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert_eq!(msg.method, "save_command");
        assert_eq!(msg.params["name"], "cmd1");
    }

    #[test]
    fn test_deserialize_remove_method() {
        let json = r#"{"jsonrpc":"2.0","method":"remove","params":{"hostname":"broker:1883"}}"#;
        let result = deserialize_json_rpc(json);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().method, "remove");
    }

    #[test]
    fn test_deserialize_invalid_json() {
        let json = "not valid json";
        let result = deserialize_json_rpc(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_empty_string() {
        let result = deserialize_json_rpc("");
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_missing_method() {
        let json = r#"{"jsonrpc":"2.0","params":{}}"#;
        let result = deserialize_json_rpc(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_missing_params() {
        let json = r#"{"jsonrpc":"2.0","method":"connect"}"#;
        let result = deserialize_json_rpc(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_with_array_params() {
        let json = r#"{"jsonrpc":"2.0","method":"test","params":[1,2,3]}"#;
        let result = deserialize_json_rpc(json);
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert_eq!(msg.params[0], 1);
        assert_eq!(msg.params[1], 2);
    }

    #[test]
    fn test_deserialize_with_nested_params() {
        let json = r#"{"jsonrpc":"2.0","method":"test","params":{"a":{"b":"c"}}}"#;
        let result = deserialize_json_rpc(json);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().params["a"]["b"], "c");
    }

    #[test]
    fn test_serialize_notification() {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0",
            method: "mqtt_message",
            params: serde_json::json!({"source": "broker:1883", "topic": "test"}),
        };
        let serialized = serde_json::to_string(&notification).unwrap();
        assert!(serialized.contains("\"jsonrpc\":\"2.0\""));
        assert!(serialized.contains("\"method\":\"mqtt_message\""));
        assert!(serialized.contains("\"source\":\"broker:1883\""));
    }

    #[test]
    fn test_roundtrip_serialize_deserialize() {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0",
            method: "connect",
            params: serde_json::json!({"hostname": "localhost:1883"}),
        };
        let serialized = serde_json::to_string(&notification).unwrap();
        let deserialized = deserialize_json_rpc(&serialized).unwrap();
        assert_eq!(deserialized.jsonrpc, "2.0");
        assert_eq!(deserialized.method, "connect");
        assert_eq!(deserialized.params["hostname"], "localhost:1883");
    }
}
