use super::entity::{Product, ProductInput};
use crate::feature::document::{entity::LineInput, validation::new_product_name};
use crate::{
    config::App,
    helper::{Result, bad},
};
use serde_json::{Value, json};

pub(crate) async fn products(a: App) -> Result<Vec<Product>> {
    Ok(sqlx::query_as("SELECT id,name,quantity,remaining_quantity,status,photo,description,price,reorder_level FROM products ORDER BY id DESC").fetch_all(&a.db).await?)
}
pub(crate) async fn save_product(a: App, p: ProductInput) -> Result<Value> {
    super::validation::validate(&p)?;
    let mut tx = a.db.begin().await?;
    let id = if p.id > 0 {
        let old: Option<(i64, i64)> = sqlx::query_as(
            "SELECT quantity,remaining_quantity FROM products WHERE id=? FOR UPDATE",
        )
        .bind(p.id)
        .fetch_optional(&mut *tx)
        .await?;
        let (physical, available) = old.ok_or_else(|| bad("Product not found"))?;
        let delta = p.quantity - physical;
        let next = p.remaining_quantity.unwrap_or(
            available
                .checked_add(delta)
                .ok_or_else(|| bad("Quantity too large"))?,
        );
        if next < 0 || next > p.quantity {
            return Err(bad(
                "Remaining quantity must be between zero and physical stock",
            ));
        }
        sqlx::query("UPDATE products SET name=?,status=?,photo=?,description=?,price=?,reorder_level=? WHERE id=?").bind(p.name.trim()).bind(&p.status).bind(&p.photo).bind(&p.description).bind(p.price).bind(p.reorder_level).bind(p.id).execute(&mut *tx).await?;
        adjust_stock(
            &mut tx,
            p.id,
            delta,
            next - available,
            None,
            "Manual adjustment",
        )
        .await?;
        p.id
    } else {
        let next = p.remaining_quantity.unwrap_or(p.quantity);
        let id=sqlx::query("INSERT INTO products(name,quantity,remaining_quantity,status,photo,description,price,reorder_level) VALUES(?,0,0,?,?,?,?,?)").bind(p.name.trim()).bind(&p.status).bind(&p.photo).bind(&p.description).bind(p.price).bind(p.reorder_level).execute(&mut *tx).await?.last_insert_id() as i64;
        adjust_stock(&mut tx, id, p.quantity, next, None, "Opening stock").await?;
        id
    };
    tx.commit().await?;
    Ok(json!({"id":id}))
}
pub(crate) async fn adjust_stock(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    product: i64,
    physical_delta: i64,
    remaining_delta: i64,
    document: Option<i64>,
    reason: &str,
) -> Result<()> {
    let (physical, available): (i64, i64) =
        sqlx::query_as("SELECT quantity,remaining_quantity FROM products WHERE id=? FOR UPDATE")
            .bind(product)
            .fetch_one(&mut **tx)
            .await?;
    let new_physical = physical
        .checked_add(physical_delta)
        .ok_or_else(|| bad("Quantity too large"))?;
    let new_available = available
        .checked_add(remaining_delta)
        .ok_or_else(|| bad("Quantity too large"))?;
    if new_physical < 0 || new_available < 0 || new_available > new_physical {
        return Err(bad(
            "Insufficient remaining quantity or invalid stock adjustment",
        ));
    }
    sqlx::query("UPDATE products SET quantity=?,remaining_quantity=? WHERE id=?")
        .bind(new_physical)
        .bind(new_available)
        .bind(product)
        .execute(&mut **tx)
        .await?;
    if physical_delta != 0 || remaining_delta != 0 {
        sqlx::query("INSERT INTO stock_movements(product_id,document_id,quantity,remaining_delta,reason) VALUES(?,?,?,?,?)").bind(product).bind(document).bind(physical_delta).bind(remaining_delta).bind(reason).execute(&mut **tx).await?;
    }
    Ok(())
}
pub(crate) async fn lookup_product(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    l: &LineInput,
) -> Result<Option<Product>> {
    if l.product_id > 0 {
        let product=sqlx::query_as("SELECT id,name,quantity,remaining_quantity,status,photo,description,price,reorder_level FROM products WHERE id=? FOR UPDATE")
            .bind(l.product_id).fetch_optional(&mut **tx).await?;
        if product.is_some() {
            return Ok(product);
        }
    }
    let name = new_product_name(l)?;
    Ok(sqlx::query_as("SELECT id,name,quantity,remaining_quantity,status,photo,description,price,reorder_level FROM products WHERE name=? FOR UPDATE")
        .bind(name).fetch_optional(&mut **tx).await?)
}
