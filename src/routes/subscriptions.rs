use actix_web::{HttpResponse, Responder, post, web};
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(serde::Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

#[tracing::instrument(
    name = "Adding a new subscriber",
    skip(form, pg_connection),
    fields(subscriber_email = %form.email, subscriber_name = %form.name)
)]
#[post("/subscriptions")]
async fn subscribe(form: web::Form<FormData>, pg_connection: web::Data<PgPool>) -> impl Responder {
    match insert_subscriber(&pg_connection, form.0).await {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[tracing::instrument(
    name = "Saving new subscriber details in the database",
    skip(pg_pool, form)
)]
async fn insert_subscriber(pg_pool: &PgPool, form: FormData) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at)
        VALUES ($1, $2, $3, $4)
        "#,
        Uuid::new_v4(),
        form.email,
        form.name,
        Utc::now()
    )
    .execute(pg_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to insert subscriber: {:?}", e);
        e
    })?;
    Ok(())
}
