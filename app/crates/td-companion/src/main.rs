#[tokio::main]
async fn main() {
    if let Err(error) = td_companion::run().await {
        eprintln!("Companion: {error}");
        std::process::exit(1);
    }
}
