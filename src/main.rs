use typojet::{api::build_router_with_auth, config::RuntimeConfig};

#[tokio::main]
async fn main() {
    let config = match RuntimeConfig::from_env_and_args() {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    };

    let app = build_router_with_auth(&config.data_dir, config.api_key.clone());
    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .expect("failed to bind address");

    println!(
        "typojet listening on http://{} using data dir {}",
        config.bind,
        config.data_dir.display()
    );
    axum::serve(listener, app)
        .await
        .expect("server exited unexpectedly");
}
