use crate::EmailClient;
use crate::domain::NewSubscriber;
use crate::startup::ApplicationBaseUrl;
use actix_web::{HttpResponse, Responder, post, web};
use chrono::Utc;
use rand::Rng;
use rand::distr::Alphanumeric;
use reqwest;
use sqlx::{PgConnection, PgPool};
use std::error::Error;
use uuid::Uuid;

#[derive(serde::Deserialize)]
pub struct FormData {
    pub email: String,
    pub name: String,
}

#[tracing::instrument(
    name = "Adding a new subscriber",
    skip(form, pg_pool, email_client, base_url),
    fields(subscriber_email = %form.email, subscriber_name = %form.name)
)]
#[post("/subscriptions")]
async fn subscribe(
    form: web::Form<FormData>,
    pg_pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    base_url: web::Data<ApplicationBaseUrl>,
) -> impl Responder {
    let mut transaction = match pg_pool.begin().await {
        Ok(db) => db,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    let subscriber: NewSubscriber = match form.0.try_into() {
        Ok(subscriber) => subscriber,
        Err(_) => return HttpResponse::BadRequest().finish(),
    };

    let subscriber_id = match insert_subscriber(&mut transaction, &subscriber).await {
        Ok(subscriber_id) => subscriber_id,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    let subscriber_token = generate_subscription_token();

    if store_token(&mut transaction, subscriber_id, &subscriber_token)
        .await
        .is_err()
    {
        return HttpResponse::InternalServerError().finish();
    }

    if transaction.commit().await.is_err() {
        return HttpResponse::InternalServerError().finish();
    }

    if send_confirm_email(&email_client, subscriber, &base_url.0, &subscriber_token)
        .await
        .is_err()
    {
        return HttpResponse::InternalServerError().finish();
    }

    HttpResponse::Ok().finish()
}
/// Generate a random 25-characters-long case-sensitive subscription token.
fn generate_subscription_token() -> String {
    let mut rng = rand::rng();
    std::iter::repeat_with(|| rng.sample(Alphanumeric))
        .map(char::from)
        .take(25)
        .collect()
}

#[tracing::instrument(
    name = "Saving new subscriber details in the database",
    skip(pg_connection, subscriber)
)]
async fn insert_subscriber(
    pg_connection: &mut PgConnection,
    subscriber: &NewSubscriber,
) -> Result<Uuid, sqlx::Error> {
    let subscriber_id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at, status)
        VALUES ($1, $2, $3, $4, $5)
        "#,
        subscriber_id,
        subscriber.email.as_ref(),
        subscriber.name.as_ref(),
        Utc::now(),
        "pending_confirmation"
    )
    .execute(pg_connection)
    .await
    .map_err(|e| {
        tracing::error!("Failed to insert subscriber: {:?}", e);
        e
    })?;
    Ok(subscriber_id)
}

#[tracing::instrument(
    name = "Store subscription token in the database",
    skip(subscription_token, pg_connection)
)]
pub async fn store_token(
    pg_connection: &mut PgConnection,
    subscriber_id: Uuid,
    subscription_token: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"INSERT INTO subscription_tokens (subscription_token, subscriber_id)
        VALUES ($1, $2)"#,
        subscription_token,
        subscriber_id
    )
    .execute(pg_connection)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;
    Ok(())
}

#[tracing::instrument(
    name = "Send a confirmation email to a new subscriber",
    skip(email_client, subscriber, base_url, subscription_token)
)]
async fn send_confirm_email(
    email_client: &EmailClient,
    subscriber: NewSubscriber,
    base_url: &str,
    subscription_token: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let confirmation_link = {
        let base = reqwest::Url::parse(base_url)?;
        let mut url = base.join("subscriptions/confirm")?;
        url.query_pairs_mut()
            .append_pair("subscription_token", subscription_token);
        url
    };
    let html = format!(
        "Welcome to our newsletter!<br />\
                Click <a href=\"{}\">here</a> to confirm your subscription.",
        confirmation_link
    );
    let text = format!(
        "Welcome to our newsletter!\nVisit {} to confirm your subscription.",
        confirmation_link
    );

    email_client
        .send_email(subscriber.email, "Welcome", &html, &text)
        .await
        .map_err(|e| {
            tracing::error!("Failed to send email: {:?}", e);
            e
        })?;
    Ok(())
}
