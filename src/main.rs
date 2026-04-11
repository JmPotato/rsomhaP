use rsomhap::App;
use tracing::error;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = match App::new().await {
        Ok(app) => app,
        Err(e) => {
            error!("failed to create app: {}", e);
            std::process::exit(1);
        }
    };
    if let Err(e) = app.serve().await {
        error!("failed to serve app: {}", e);
        std::process::exit(1);
    }
}
