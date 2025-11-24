mod cli;
mod infrastructure;
mod server;

use cli::Cli;

#[tokio::main()]
async fn main() {
    let cli = Cli::initialize();
    let settings = cli.load_settings().unwrap();

    cli.handle(settings).await.unwrap();
}
