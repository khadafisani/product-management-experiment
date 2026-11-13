pub(crate) async fn connect() -> Result<sqlx::MySqlPool, Box<dyn std::error::Error>> {
    let db = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(10)
        .after_connect(|conn, _| {
            Box::pin(async move {
                sqlx::query("SET time_zone = '+07:00'")
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect(&std::env::var("DATABASE_URL").expect("Set DATABASE_URL in backend/.env"))
        .await?;
    sqlx::migrate!().run(&db).await?;
    Ok(db)
}
