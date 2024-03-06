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

pub fn get_known_brokers(brokers_path: &str) -> VecDeque<String> {
    if let Ok(file_content) = &std::fs::read_to_string(&brokers_path) {
        serde_json::from_str(&file_content).unwrap_or_else(|_| VecDeque::new())
    } else {
        eprintln!("Failed to read file {}", &brokers_path);
        VecDeque::new()
    }
}

pub fn add_to_brokers(brokers_path: &str, broker: &str) {
    let mut brokers: Vec<String> = if let Ok(file_content) = std::fs::read_to_string(brokers_path) {
        serde_json::from_str(&file_content).unwrap_or_else(|_| Vec::new())
    } else {
        eprintln!("Failed to read file {}", brokers_path);
        Vec::new()
    };

    brokers.push(broker.to_string());
    if let Ok(content) = serde_json::to_string(&brokers) {
        if let Err(_) = std::fs::write(brokers_path, content) {
            eprintln!("Failed to save new brokers file to {}", brokers_path);
        }
    } else {
        eprintln!("Failed to serialize updated saved brokers.");
    }
}

pub fn remove_from_brokers(brokers_path: &str, broker: &str) {
    let mut brokers: Vec<String> = if let Ok(file_content) = std::fs::read_to_string(brokers_path) {
        serde_json::from_str(&file_content).unwrap_or_else(|_| Vec::new())
    } else {
        eprintln!("Failed to read file {}", brokers_path);
        Vec::new()
    };

    if let Some(index) = brokers.iter().position(|b| b == broker) {
        brokers.remove(index);
        if let Ok(content) = serde_json::to_string(&brokers) {
            if let Err(_) = std::fs::write(brokers_path, content) {
                eprintln!("Failed to save new brokers file to {}", brokers_path);
            }
        } else {
            eprintln!("Failed to serialize updated saved brokers.");
        }
    } else {
        eprintln!("Broker {} not found in {}", broker, brokers_path);
    }
}

pub fn add_to_commands(commands_path: &str, params: serde_json::Value) {
    if let Ok(new_command) = serde_json::from_value::<CommandMessage>(params) {
        let new_command_path = std::format!("{}/{}.json", commands_path, new_command.name);
        if let Some(parent_dir) = std::path::Path::new(&new_command_path).parent() {
            std::fs::create_dir_all(parent_dir).expect("Failed to create directory path");
        }
        if let Ok(content) = serde_json::to_string(&new_command) {
            if let Err(_) = std::fs::write(&new_command_path, content) {
                eprintln!("Failed to save new commands file to {}", new_command_path);
            }
        } else {
            eprintln!("Failed to serialize updated saved commands.");
        }
    } else {
        println!("Could not deserialize new command.");
    }
}

pub fn remove_from_commands(commands_path: &String, params: serde_json::Value) {
    let command = params["name"].as_str().unwrap();
    let command_path = std::format!("{}/{}.json", commands_path, command);
    if let Err(_) = std::fs::remove_file(&command_path) {
        eprintln!("Failed to remove command file from {}", command_path);
    }
}

pub fn add_to_pipelines(pipelines_path: &String, params: serde_json::Value) {
    if let Ok(new_pipeline) = serde_json::from_value::<PipelineMessage>(params) {
        let new_pipeline_path = std::format!("{}/{}.json", pipelines_path, new_pipeline.name);
        if let Some(parent_dir) = std::path::Path::new(&new_pipeline_path).parent() {
            std::fs::create_dir_all(parent_dir).expect("Failed to create directory path");
        }
        if let Ok(content) = serde_json::to_string(&new_pipeline) {
            if let Err(_) = std::fs::write(&new_pipeline_path, content) {
                eprintln!("Failed to save new commands file to {}", new_pipeline_path);
            }
        } else {
            eprintln!("Failed to serialize updated saved commands.");
        }
    } else {
        println!("Could not deserialize new pipeline.");
    }
}

pub fn remove_from_pipelines(pipelines_path: &String, params: serde_json::Value) {
    let pipeline = params["name"].as_str().unwrap();
    let pipeline_path = std::format!("{}/{}.json", pipelines_path, pipeline);
    if let Err(_) = std::fs::remove_file(&pipeline_path) {
        eprintln!("Failed to remove pipeline file from {}", pipeline_path);
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
        assert_eq!(brokers[0], "localhost:1883");
        assert_eq!(brokers[1], "127.0.0.1:1234");
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
        let broker = "test.mosquitto.org:1883";
        let brokers = get_known_brokers(&resource.brokers_path);
        let len_before = brokers.len();

        add_to_brokers(&resource.brokers_path, broker);
        let brokers = get_known_brokers(&resource.brokers_path);

        assert_eq!(brokers.len(), len_before + 1);
        assert_eq!(brokers.back().unwrap(), "test.mosquitto.org:1883");
    }

    #[test]
    fn test_add_to_brokers_no_file() {
        let resource = TestResource::new();
        let broker = "test.mosquitto.org:1883";
        let brokers_path = std::format!("{}/brokers.json", resource.config_path);
        let brokers = get_known_brokers(&brokers_path);
        let len_before = brokers.len();

        add_to_brokers(&brokers_path, broker);
        let brokers = get_known_brokers(&brokers_path);

        assert_eq!(brokers.len(), len_before + 1);
        assert_eq!(brokers.back().unwrap(), "test.mosquitto.org:1883");
    }

    #[test]
    fn test_remove_from_brokers() {
        let resource = TestResource::new();
        let brokers_path = std::format!("{}/brokers.json", resource.config_path);
        let brokers = get_known_brokers(brokers_path.as_str());
        let len_before = brokers.len();

        let broker = brokers.front().unwrap();
        remove_from_brokers(brokers_path.as_str(), broker);
        let brokers: VecDeque<String> = get_known_brokers(brokers_path.as_str());

        assert_eq!(brokers.len(), len_before - 1);
        assert_ne!(brokers.front().unwrap(), broker);
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
        // TODO remove_from_brokers should return an Err
        remove_from_brokers("not_a_real_path.json", broker);

        // Just to check that this did not fail
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
        assert_eq!(std::path::Path::new(&command_path.clone()).exists(), true);
        let old_command = std::fs::read_to_string(command_path.clone()).unwrap();
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
        assert_eq!(std::path::Path::new(&command_path.clone()).exists(), true);
        let params = serde_json::json!({
            "name": "first_command"
        });
        remove_from_commands(&resource.commands_path.to_string(), params);
        assert!(std::fs::metadata(command_path).is_err());
    }

    #[test]
    fn test_remove_from_commands_failure_no_file() {
        let params = serde_json::json!({
            "name": "does_not_exist"
        });
        remove_from_commands(&"whatever".to_string(), params);
        // TODO: Should return error instead
        assert!(true);
    }

    #[test]
    fn test_add_to_pipelines() {
        let resource = TestResource::new();
        let pipeline_path = std::format!("{}/new_pipeline.json", resource.pipelines_path);
        assert_eq!(std::path::Path::new(&pipeline_path.clone()).exists(), false);
        let params = serde_json::json!({
            "name": "new_pipeline",
            "pipeline": [
                {
                    "topic": "test"
                }
            ]
        });
        add_to_pipelines(&resource.pipelines_path.to_string(), params);
        let pipeline = std::fs::read_to_string(pipeline_path.clone()).unwrap();
        let pipeline: PipelineMessage = serde_json::from_str(&pipeline).unwrap();
        assert_eq!(pipeline.name, "new_pipeline");
        assert_eq!(pipeline.pipeline[0].topic, "test");
        assert_eq!(std::path::Path::new(&pipeline_path.clone()).exists(), true);
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
        add_to_pipelines(&resource.pipelines_path.to_string(), params);
        let pipeline_path = std::format!("{}/failure_mode.json", resource.pipelines_path);
        assert!(std::fs::metadata(pipeline_path).is_err());
    }

    #[test]
    fn test_remove_from_pipelines() {
        let resource = TestResource::new();
        let pipeline_path = std::format!("{}/empty_pipeline.json", resource.pipelines_path);
        assert_eq!(std::path::Path::new(&pipeline_path.clone()).exists(), true);
        let params = serde_json::json!({
            "name": "test"
        });
        remove_from_pipelines(&resource.pipelines_path.to_string(), params);
        let pipeline_path = std::format!("{}/test.json", resource.pipelines_path);
        assert!(std::fs::metadata(pipeline_path.clone()).is_err());
        assert_eq!(std::path::Path::new(&pipeline_path.clone()).exists(), false);
    }

    #[test]
    fn test_remove_from_pipelines_failure() {
        let resource = TestResource::new();
        let pipelines_path = std::format!("{}/pipelines", resource.config_path);
        let params = serde_json::json!({
            "name": "sould_not_exist"
        });
        remove_from_pipelines(&pipelines_path.to_string(), params);
        let pipeline_path = std::format!("{}/test.json", pipelines_path);
        assert!(std::fs::metadata(pipeline_path).is_err());
        assert!(true);
    }
}
