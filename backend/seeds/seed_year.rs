//! Run from backend: cargo run --bin seed_year
use chrono::{FixedOffset, NaiveDate, Utc};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let mut end = Utc::now()
        .with_timezone(&FixedOffset::east_opt(7 * 3600).unwrap())
        .date_naive();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--end-date" => {
                end = NaiveDate::parse_from_str(
                    &args.next().ok_or("--end-date requires YYYY-MM-DD")?,
                    "%Y-%m-%d",
                )?;
            }
            "--help" => {
                println!(
                    "cargo run --bin seed_year -- [--end-date YYYY-MM-DD]\nAdds one trailing year of demo history once; preserves all existing data."
                );
                return Ok(());
            }
            _ => return Err(format!("Unknown argument: {arg}").into()),
        }
    }
    let db = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(3)
        .after_connect(|c, _| {
            Box::pin(async move {
                sqlx::query("SET time_zone='+07:00'").execute(c).await?;
                Ok(())
            })
        })
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;
    sqlx::migrate!().run(&db).await?;
    backend::seeds::auth::seed_auth(&db).await?;
    let result = backend::seeds::year::seed_year(&db, end).await?;
    println!(
        "{}",
        if result.inserted {
            "Year demo committed; all reconciliation checks passed."
        } else {
            "Year demo already exists; no business records changed."
        }
    );
    println!("{}", serde_json::to_string_pretty(&result.report)?);
    db.close().await;
    Ok(())
}
