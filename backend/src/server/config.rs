/*
 * Copyright (c) 2024 Kai Lawrence
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

use std::collections::VecDeque;

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
pub struct BrokerConfig {
    pub host: String,
    #[serde(default)]
    pub use_tls: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

impl BrokerConfig {
    /// The display key used to identify this broker throughout the app (host:port).
    pub fn key(&self) -> &str {
        &self.host
    }

    #[cfg(test)]
    pub fn from_host(host: &str) -> Self {
        Self {
            host: host.to_string(),
            use_tls: false,
            username: None,
            password: None,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct CommandMessage {
    name: String,
    topic: String,
    payload: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct PipelineEntry {
    topic: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct PipelineMessage {
    name: String,
    pipeline: VecDeque<PipelineEntry>,
}

pub fn get_known_brokers(brokers_path: &str) -> VecDeque<BrokerConfig> {
    if let Ok(file_content) = std::fs::read_to_string(brokers_path) {
        serde_json::from_str(&file_content).unwrap_or_else(|_| VecDeque::new())
    } else {
        eprintln!("Failed to read file {}", brokers_path);
        VecDeque::new()
    }
}

fn read_broker_configs(brokers_path: &str) -> Vec<BrokerConfig> {
    if let Ok(file_content) = std::fs::read_to_string(brokers_path) {
        serde_json::from_str(&file_content).unwrap_or_else(|_| Vec::new())
    } else {
        Vec::new()
    }
}

fn write_broker_configs(brokers_path: &str, configs: &[BrokerConfig]) {
    match serde_json::to_string_pretty(configs) {
        Ok(content) => {
            if std::fs::write(brokers_path, content).is_err() {
                eprintln!("Failed to save brokers file to {brokers_path}");
            }
        }
        Err(_) => eprintln!("Failed to serialize broker configs."),
    }
}

pub fn add_to_brokers(brokers_path: &str, config: &BrokerConfig) {
    let mut brokers = read_broker_configs(brokers_path);
    brokers.push(config.clone());
    write_broker_configs(brokers_path, &brokers);
}

pub fn remove_from_brokers(brokers_path: &str, broker: &str) {
    let mut brokers = read_broker_configs(brokers_path);
    if let Some(index) = brokers.iter().position(|b| b.host == broker) {
        brokers.remove(index);
        write_broker_configs(brokers_path, &brokers);
    } else {
        eprintln!("Broker {broker} not found in {brokers_path}");
    }
}

pub fn add_to_commands(commands_path: &str, params: serde_json::Value) {
    if let Ok(new_command) = serde_json::from_value::<CommandMessage>(params) {
        let new_command_path = std::format!("{commands_path}/{}.json", new_command.name);
        if let Some(parent_dir) = std::path::Path::new(&new_command_path).parent() {
            std::fs::create_dir_all(parent_dir).expect("Failed to create directory path");
        }
        if let Ok(content) = serde_json::to_string(&new_command) {
            if std::fs::write(&new_command_path, content).is_err() {
                eprintln!("Failed to save new commands file to {new_command_path}");
            }
        } else {
            eprintln!("Failed to serialize updated saved commands.");
        }
    } else {
        println!("Could not deserialize new command.");
    }
}

pub fn remove_from_commands(commands_path: &str, params: serde_json::Value) {
    let command = match params.get("name").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => {
            eprintln!("Missing or invalid 'name' param for remove_command");
            return;
        }
    };
    let command_path = std::format!("{commands_path}/{command}.json");
    if std::fs::remove_file(&command_path).is_err() {
        eprintln!("Failed to remove command file from {command_path}");
    }
}

pub fn add_to_pipelines(pipelines_path: &str, params: serde_json::Value) {
    if let Ok(new_pipeline) = serde_json::from_value::<PipelineMessage>(params) {
        let new_pipeline_path = std::format!("{pipelines_path}/{}.json", new_pipeline.name);
        if let Some(parent_dir) = std::path::Path::new(&new_pipeline_path).parent() {
            std::fs::create_dir_all(parent_dir).expect("Failed to create directory path");
        }
        if let Ok(content) = serde_json::to_string(&new_pipeline) {
            if std::fs::write(&new_pipeline_path, content).is_err() {
                eprintln!("Failed to save new commands file to {new_pipeline_path}");
            }
        } else {
            eprintln!("Failed to serialize updated saved commands.");
        }
    } else {
        println!("Could not deserialize new pipeline.");
    }
}

pub fn remove_from_pipelines(pipelines_path: &str, params: serde_json::Value) {
    let pipeline = match params.get("name").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            eprintln!("Missing or invalid 'name' param for remove_pipeline");
            return;
        }
    };
    let pipeline_path = std::format!("{pipelines_path}/{pipeline}.json");
    if std::fs::remove_file(&pipeline_path).is_err() {
        eprintln!("Failed to remove pipeline file from {pipeline_path}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG_PATH: &str = "../test/config";
    const CONFIG_SOURCE_PATH: &str = "../test/config_source";

    struct TestResource {
        config_path: String,
        brokers_path: String,
        commands_path: String,
        pipelines_path: String,
    }

    impl TestResource {
        fn new() -> Self {
            let config_path =
                std::format!("{}_{}", CONFIG_PATH, uuid::Uuid::new_v4().to_string()).to_string();
            copy_dir::copy_dir(CONFIG_SOURCE_PATH, config_path.to_string()).unwrap();
            let brokers_path = std::format!("{}/brokers.json", config_path);
            let commands_path = std::format!("{}/commands", config_path);
            let pipelines_path = std::format!("{}/pipelines", config_path);

            Self {
                config_path,
                brokers_path,
                commands_path,
                pipelines_path,
            }
        }
    }

    impl Drop for TestResource {
        fn drop(&mut self) {
            if std::path::Path::new(&self.config_path).exists() {
                if let Err(err) = std::fs::remove_dir_all(&self.config_path) {
                    eprintln!("Failed to remove directory: {:?}", err);
                }
            }
        }
    }

    #[test]
    fn test_get_known_brokers() {
        let resource = TestResource::new();
        let brokers =
            get_known_brokers(std::format!("{}/brokers.json", resource.config_path).as_str());
        assert_eq!(brokers.len(), 2);
        assert_eq!(brokers[0].host, "localhost:1883");
        assert_eq!(brokers[1].host, "127.0.0.1:1234");
    }

    #[test]
    fn test_get_known_brokers_failure() {
        let brokers =
            get_known_brokers(std::format!("{}/should_not_exist.json", CONFIG_PATH).as_str());
        assert_eq!(brokers.len(), 0);
    }

    #[test]
    fn test_add_to_brokers() {
        let resource = TestResource::new();
        let broker = BrokerConfig::from_host("test.mosquitto.org:1883");
        let brokers = get_known_brokers(&resource.brokers_path);
        let len_before = brokers.len();

        add_to_brokers(&resource.brokers_path, &broker);
        let brokers = get_known_brokers(&resource.brokers_path);

        assert_eq!(brokers.len(), len_before + 1);
        assert_eq!(brokers.back().unwrap().host, "test.mosquitto.org:1883");
    }

    #[test]
    fn test_add_to_brokers_no_file() {
        let resource = TestResource::new();
        let broker = BrokerConfig::from_host("test.mosquitto.org:1883");
        let brokers_path = std::format!("{}/brokers.json", resource.config_path);
        let brokers = get_known_brokers(&brokers_path);
        let len_before = brokers.len();

        add_to_brokers(&brokers_path, &broker);
        let brokers = get_known_brokers(&brokers_path);

        assert_eq!(brokers.len(), len_before + 1);
        assert_eq!(brokers.back().unwrap().host, "test.mosquitto.org:1883");
    }

    #[test]
    fn test_remove_from_brokers() {
        let resource = TestResource::new();
        let brokers_path = std::format!("{}/brokers.json", resource.config_path);
        let brokers = get_known_brokers(brokers_path.as_str());
        let len_before = brokers.len();

        let broker_host = brokers.front().unwrap().host.clone();
        remove_from_brokers(brokers_path.as_str(), &broker_host);
        let brokers = get_known_brokers(brokers_path.as_str());

        assert_eq!(brokers.len(), len_before - 1);
        assert_ne!(brokers.front().unwrap().host, broker_host);
    }

    #[test]
    fn test_remove_from_brokers_failure() {
        let resource = TestResource::new();
        let brokers_path = std::format!("{}/brokers.json", resource.config_path);
        let broker = "does_not_exist:1883";
        let brokers_before = get_known_brokers(brokers_path.as_str());
        let len_before = brokers_before.len();

        remove_from_brokers(brokers_path.as_str(), broker);
        let brokers_after = get_known_brokers(brokers_path.as_str());
        let len_after = brokers_after.len();

        assert_eq!(len_before, len_after);
    }

    #[test]
    fn test_remove_from_brokers_failure_no_file() {
        let broker = "test.mosquitto.org:1883";
        remove_from_brokers("not_a_real_path.json", broker);
        assert!(true);
    }

    #[test]
    fn test_add_to_commands() {
        let resource = TestResource::new();
        let params = serde_json::json!({
            "name": "new_command",
            "topic": "test",
            "payload": "test"
        });
        let command_path = std::format!("{}/new_command.json", resource.commands_path);
        assert_eq!(std::path::Path::new(&command_path).exists(), false);
        add_to_commands(&resource.commands_path, params);
        assert_eq!(std::path::Path::new(&resource.commands_path).exists(), true);
        let command = std::fs::read_to_string(command_path).unwrap();
        let command: CommandMessage = serde_json::from_str(&command).unwrap();
        assert_eq!(command.name, "new_command");
        assert_eq!(command.topic, "test");
        assert_eq!(command.payload, "test");
    }

    #[test]
    fn test_add_to_commands_already_exists() {
        let resource = TestResource::new();
        let command_path = std::format!("{}/first_command.json", resource.commands_path);
        assert!(std::path::Path::new(&command_path).exists());
        let old_command = std::fs::read_to_string(&command_path).unwrap();
        let old_command: CommandMessage = serde_json::from_str(&old_command).unwrap();
        assert_eq!(old_command.name, "first_command");
        assert_eq!(old_command.topic, "test");
        assert_eq!(old_command.payload, "I am a test");

        let params = serde_json::json!({
            "name": "first_command",
            "topic": "replaced",
            "payload": "I replaced the test"
        });

        add_to_commands(&resource.commands_path, params);
        let new_command = std::fs::read_to_string(&command_path).unwrap();
        let new_command: CommandMessage = serde_json::from_str(&new_command).unwrap();
        assert_eq!(new_command.name, "first_command");
        assert_eq!(new_command.topic, "replaced");
        assert_eq!(new_command.payload, "I replaced the test");
    }

    #[test]
    fn test_remove_from_commands() {
        let resource = TestResource::new();
        let command_path = std::format!("{}/first_command.json", resource.commands_path);
        assert!(std::path::Path::new(&command_path).exists());
        let params = serde_json::json!({
            "name": "first_command"
        });
        remove_from_commands(&resource.commands_path, params);
        assert!(std::fs::metadata(command_path).is_err());
    }

    #[test]
    fn test_remove_from_commands_failure_no_file() {
        let params = serde_json::json!({
            "name": "does_not_exist"
        });
        remove_from_commands("whatever", params);
        assert!(true);
    }

    #[test]
    fn test_add_to_pipelines() {
        let resource = TestResource::new();
        let pipeline_path = std::format!("{}/new_pipeline.json", resource.pipelines_path);
        assert!(!std::path::Path::new(&pipeline_path).exists());
        let params = serde_json::json!({
            "name": "new_pipeline",
            "pipeline": [
                {
                    "topic": "test"
                }
            ]
        });
        add_to_pipelines(&resource.pipelines_path, params);
        let pipeline = std::fs::read_to_string(&pipeline_path).unwrap();
        let pipeline: PipelineMessage = serde_json::from_str(&pipeline).unwrap();
        assert_eq!(pipeline.name, "new_pipeline");
        assert_eq!(pipeline.pipeline[0].topic, "test");
        assert!(std::path::Path::new(&pipeline_path).exists());
    }

    #[test]
    fn test_add_to_pipelines_failure() {
        let resource = TestResource::new();
        let params = serde_json::json!({
            "name": "failure_mode",
            "pipeline": [
                {
                    "wrong_key": "test"
                }
            ]
        });
        add_to_pipelines(&resource.pipelines_path, params);
        let pipeline_path = std::format!("{}/failure_mode.json", resource.pipelines_path);
        assert!(std::fs::metadata(pipeline_path).is_err());
    }

    #[test]
    fn test_remove_from_pipelines() {
        let resource = TestResource::new();
        let pipeline_path = std::format!("{}/empty_pipeline.json", resource.pipelines_path);
        assert!(std::path::Path::new(&pipeline_path).exists());
        let params = serde_json::json!({
            "name": "test"
        });
        remove_from_pipelines(&resource.pipelines_path, params);
        let pipeline_path = std::format!("{}/test.json", resource.pipelines_path);
        assert!(std::fs::metadata(&pipeline_path).is_err());
        assert!(!std::path::Path::new(&pipeline_path).exists());
    }

    #[test]
    fn test_remove_from_pipelines_failure() {
        let resource = TestResource::new();
        let pipelines_path = std::format!("{}/pipelines", resource.config_path);
        let params = serde_json::json!({
            "name": "sould_not_exist"
        });
        remove_from_pipelines(&pipelines_path, params);
        let pipeline_path = std::format!("{}/test.json", pipelines_path);
        assert!(std::fs::metadata(pipeline_path).is_err());
        assert!(true);
    }

    // --- Tests for missing/invalid "name" parameter ---

    #[test]
    fn test_remove_from_commands_missing_name_param() {
        let resource = TestResource::new();
        // No "name" key at all — should not panic, just early return
        let params = serde_json::json!({});
        remove_from_commands(&resource.commands_path, params);
        // Verify no files were deleted (first_command still exists)
        let command_path = std::format!("{}/first_command.json", resource.commands_path);
        assert!(std::path::Path::new(&command_path).exists());
    }

    #[test]
    fn test_remove_from_commands_wrong_key() {
        let resource = TestResource::new();
        // Has a key, but not "name"
        let params = serde_json::json!({"wrong_key": "first_command"});
        remove_from_commands(&resource.commands_path, params);
        let command_path = std::format!("{}/first_command.json", resource.commands_path);
        assert!(std::path::Path::new(&command_path).exists());
    }

    #[test]
    fn test_remove_from_commands_name_is_not_string() {
        let resource = TestResource::new();
        // "name" exists but is a number, not a string
        let params = serde_json::json!({"name": 42});
        remove_from_commands(&resource.commands_path, params);
        let command_path = std::format!("{}/first_command.json", resource.commands_path);
        assert!(std::path::Path::new(&command_path).exists());
    }

    #[test]
    fn test_remove_from_commands_name_is_null() {
        let resource = TestResource::new();
        let params = serde_json::json!({"name": null});
        remove_from_commands(&resource.commands_path, params);
        let command_path = std::format!("{}/first_command.json", resource.commands_path);
        assert!(std::path::Path::new(&command_path).exists());
    }

    #[test]
    fn test_remove_from_pipelines_missing_name_param() {
        let resource = TestResource::new();
        let params = serde_json::json!({});
        remove_from_pipelines(&resource.pipelines_path, params);
        let pipeline_path = std::format!("{}/empty_pipeline.json", resource.pipelines_path);
        assert!(std::path::Path::new(&pipeline_path).exists());
    }

    #[test]
    fn test_remove_from_pipelines_wrong_key() {
        let resource = TestResource::new();
        let params = serde_json::json!({"wrong_key": "empty_pipeline"});
        remove_from_pipelines(&resource.pipelines_path, params);
        let pipeline_path = std::format!("{}/empty_pipeline.json", resource.pipelines_path);
        assert!(std::path::Path::new(&pipeline_path).exists());
    }

    #[test]
    fn test_remove_from_pipelines_name_is_not_string() {
        let resource = TestResource::new();
        let params = serde_json::json!({"name": 123});
        remove_from_pipelines(&resource.pipelines_path, params);
        let pipeline_path = std::format!("{}/empty_pipeline.json", resource.pipelines_path);
        assert!(std::path::Path::new(&pipeline_path).exists());
    }

    #[test]
    fn test_remove_from_pipelines_name_is_null() {
        let resource = TestResource::new();
        let params = serde_json::json!({"name": null});
        remove_from_pipelines(&resource.pipelines_path, params);
        let pipeline_path = std::format!("{}/empty_pipeline.json", resource.pipelines_path);
        assert!(std::path::Path::new(&pipeline_path).exists());
    }

    // --- add_to_commands edge case: invalid JSON params ---

    #[test]
    fn test_add_to_commands_invalid_params() {
        let resource = TestResource::new();
        // Missing required fields — should not panic
        let params = serde_json::json!({"name": "broken"});
        add_to_commands(&resource.commands_path, params);
        // File should not be created since deserialization fails
        let command_path = std::format!("{}/broken.json", resource.commands_path);
        assert!(!std::path::Path::new(&command_path).exists());
    }
}
