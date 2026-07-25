#[tokio::main]
async fn main() {
    if let Err(error) = network_cartographer_lib::run().await {
        eprintln!("netcart: {error}");
        std::process::exit(1);
    }
}
