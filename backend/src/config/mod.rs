mod database;
pub(crate) mod router;
mod state;
pub(crate) use state::App;
use std::env;
pub async fn run() -> std::result::Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    crate::feature::auth::validate_config()?;
    let db = database::connect().await?;
    let app = App {
        db,
        http: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()?,
        meili: env::var("MEILI_URL")
            .unwrap_or("http://127.0.0.1:7700".into())
            .trim_end_matches('/')
            .into(),
        key: env::var("MEILI_KEY").unwrap_or_default(),
    };
    let cleanup_app = app.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            if let Err(e) = crate::feature::auth::cleanup(&cleanup_app).await {
                eprintln!("Auth cleanup: {}", e.1);
            }
        }
    });
    let sync_app = app.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            if let Err(e) = crate::feature::search::service::reindex(sync_app.clone()).await {
                eprintln!("Search sync: {}", e.1);
            }
        }
    });
    let origin = env::var("FRONTEND_ORIGIN")
        .unwrap_or("http://localhost:5173".into())
        .parse::<axum::http::HeaderValue>()?;
    let router = router::router(app, origin);
    let bind = env::var("BIND_ADDRESS").unwrap_or("127.0.0.1:3000".into());
    println!("Product Management API listening on {bind}");
    axum::serve(tokio::net::TcpListener::bind(bind).await?, router).await?;
    Ok(())
}
