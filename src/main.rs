mod cli;
mod server;

use cli::Cli;

#[tokio::main()]
async fn main() {
    let cli = Cli::initialize();

    cli.handle().await.unwrap();
}
