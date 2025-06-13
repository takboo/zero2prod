use crate::EmailClient;
use crate::domain::NewSubscriber;
use crate::startup::ApplicationBaseUrl;
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, ResponseError, post, web};
use anyhow::Context;
use chrono::Utc;
use rand::Rng;
use rand::distr::Alphanumeric;
use reqwest;
use sqlx::{PgConnection, PgPool};
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
) -> Result<HttpResponse, SubscribeError> {
    let mut transaction = pg_pool
        .begin()
        .await
        .context("Failed to acquire a Postgres connection from the pool")?;

    let subscriber: NewSubscriber = form.0.try_into().map_err(SubscribeError::ValidationError)?;

    let subscriber_id = insert_subscriber(&mut transaction, &subscriber)
        .await
        .context("Failed to insert new subscriber in the database")?;

    let subscriber_token = generate_subscription_token();

    store_token(&mut transaction, subscriber_id, &subscriber_token)
        .await
        .context("Failed to store the confirmation token for a new subscriber")?;

    transaction
        .commit()
        .await
        .context("Failed to commit SQL transaction to store a new subscriber")?;

    let confirmation_link = create_confirmation_link(&base_url.0, &subscriber_token)
        .context("Failed to create a confirmation link for a new subscriber")?;

    send_confirm_email(&email_client, subscriber, confirmation_link)
        .await
        .context("Failed to send the confirmation email")?;

    Ok(HttpResponse::Ok().finish())
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
    .await?;
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
    .await?;
    Ok(())
}

#[tracing::instrument(
    name = "Create new confirmation link for new subscriber",
    skip(base_url)
)]
fn create_confirmation_link(
    base_url: &str,
    subscription_token: &str,
) -> Result<url::Url, url::ParseError> {
    let base = url::Url::parse(base_url)?;
    let mut url = base.join("subscriptions/confirm")?;
    url.query_pairs_mut()
        .append_pair("subscription_token", subscription_token);
    Ok(url)
}

#[tracing::instrument(
    name = "Send a confirmation email to a new subscriber",
    skip(email_client, subscriber, confirmation_link)
)]
async fn send_confirm_email(
    email_client: &EmailClient,
    subscriber: NewSubscriber,
    confirmation_link: url::Url,
) -> Result<(), reqwest::Error> {
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
        .send_email(&subscriber.email, "Welcome", &html, &text)
        .await?;
    Ok(())
}

#[derive(thiserror::Error)]
pub enum SubscribeError {
    #[error("{0}")]
    ValidationError(String),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for SubscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for SubscribeError {
    fn status_code(&self) -> StatusCode {
        match self {
            SubscribeError::ValidationError(_) => StatusCode::BAD_REQUEST,
            SubscribeError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

pub fn error_chain_fmt(
    e: &(dyn std::error::Error + 'static),
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{}", e)?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Cause by:\n\t {}", cause)?;
        current = cause.source();
    }
    Ok(())
}
