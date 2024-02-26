mod config;
mod jsonrpc;
mod mqtt;
mod server;

fn get_arguments_or_default() -> (String, String) {
    let args: Vec<String> = std::env::args().collect();
    let static_files = if args.len() < 2 {
        "../frontend/wwwroot".to_string()
    } else {
        args[1].clone()
    };
    let config_dir = if args.len() < 3 {
        "../test/config".to_string()
    } else {
        args[2].clone()
    };

    (static_files, config_dir)
}

#[tokio::main]
async fn main() -> () {
    let (static_files, config_dir) = get_arguments_or_default();

    let warp_handle = server::run_server(static_files, config_dir);

    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for event");
    println!("\nReceived ctrl-c. Shutting down");
    warp_handle.abort();
}
