
mod jsonrpc;
mod mqtt;
mod server;

#[tokio::main]
async fn main() -> () {
    let args: Vec<String> = std::env::args().collect();
    let static_files = if args.len() < 2 {
        "../frontend/wwwroot".to_string()
    } else {
        args[1].clone()
    };

    let warp_handle = server::run(static_files);

    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for event");
    println!("\nReceived ctrl-c. Shutting down");
    warp_handle.abort();
}
