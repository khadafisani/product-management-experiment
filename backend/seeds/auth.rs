//! Explicit demo accounts; existing passwords and role assignments are preserved.
use argon2::{
    Argon2, PasswordHasher,
    password_hash::{SaltString, rand_core::OsRng},
};
pub async fn seed_auth(db: &sqlx::MySqlPool) -> Result<(), Box<dyn std::error::Error>> {
    let permissions = [
        "profile.read",
        "roles.manage",
        "payments.read",
        "payments.write",
        "invoices.read",
        "invoices.write",
        "invoices.request_void",
        "invoices.void",
        "delivery.read",
        "delivery.write",
        "delivery.approve",
        "products.read",
        "products.write",
        "search.manage",
    ];
    let mut tx = db.begin().await?;
    for p in permissions {
        sqlx::query("INSERT IGNORE INTO permissions(name) VALUES(?)")
            .bind(p)
            .execute(&mut *tx)
            .await?;
    }
    for role in ["Admin", "Staff", "Finance"] {
        let inserted = sqlx::query("INSERT IGNORE INTO roles(name) VALUES(?)")
            .bind(role)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            > 0;
        let id: i64 = sqlx::query_scalar("SELECT id FROM roles WHERE name=?")
            .bind(role)
            .fetch_one(&mut *tx)
            .await?;
        for p in permissions {
            let grant = match role {
                "Admin" => true,
                "Staff" => ![
                    "roles.manage",
                    "payments.write",
                    "invoices.void",
                    "delivery.approve",
                    "products.write",
                    "search.manage",
                ]
                .contains(&p),
                _ => ![
                    "roles.manage",
                    "invoices.write",
                    "products.write",
                    "search.manage",
                ]
                .contains(&p),
            };
            if grant && (inserted || role == "Admin") {
                sqlx::query("INSERT INTO role_permissions(role_id,permission) VALUES(?,?) ON DUPLICATE KEY UPDATE deleted_at=NULL")
                    .bind(id)
                    .bind(p)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        let hash = Argon2::default()
            .hash_password(b"asdqwe123!", &SaltString::generate(&mut OsRng))
            .map_err(|e| e.to_string())?
            .to_string();
        sqlx::query("INSERT IGNORE INTO users(username,password_hash,role_id) VALUES(?,?,?)")
            .bind(role.to_lowercase())
            .bind(hash)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}
