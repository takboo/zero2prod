use crate::routes::error_chain_fmt;
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, ResponseError, get, web};
use anyhow::Context;
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct ConfirmRequest {
    subscription_token: String,
}

#[derive(thiserror::Error)]
pub enum SubscriptionConfirmError {
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
    #[error("There is no subscriber associated with the provided token.")]
    UnknownToken,
}

impl std::fmt::Debug for SubscriptionConfirmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for SubscriptionConfirmError {
    fn status_code(&self) -> StatusCode {
        match self {
            SubscriptionConfirmError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            SubscriptionConfirmError::UnknownToken => StatusCode::UNAUTHORIZED,
        }
    }
}

#[tracing::instrument(name = "Confirm a pending subscriber", skip(confirm_request, pg_pool))]
#[get("/subscriptions/confirm")]
pub async fn confirm(
    confirm_request: web::Query<ConfirmRequest>,
    pg_pool: web::Data<PgPool>,
) -> Result<HttpResponse, SubscriptionConfirmError> {
    let id = get_subscriber_id_from_token(&pg_pool, &confirm_request.subscription_token)
        .await
        .context(format!(
            "Failed to retrieve the subscriber id associated with the provided token {}",
            confirm_request.subscription_token
        ))?
        .ok_or(SubscriptionConfirmError::UnknownToken)?;
    confirm_subscriber(&pg_pool, id)
        .await
        .context("Failed to update the subscriber status to `confirmed`.")?;
    Ok(HttpResponse::Ok().finish())
}

#[tracing::instrument(name = "Mark subscriber as confirmed", skip(subscriber_id, pg_pool))]
pub async fn confirm_subscriber(pg_pool: &PgPool, subscriber_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE subscriptions SET status = 'confirmed' WHERE id = $1"#,
        subscriber_id,
    )
    .execute(pg_pool)
    .await?;
    Ok(())
}

#[tracing::instrument(
    name = "Get subscriber_id from token",
    skip(subscription_token, pg_pool)
)]
pub async fn get_subscriber_id_from_token(
    pg_pool: &PgPool,
    subscription_token: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    let result = sqlx::query!(
        r#"SELECT subscriber_id FROM subscription_tokens WHERE subscription_token = $1"#,
        subscription_token,
    )
    .fetch_optional(pg_pool)
    .await?;
    Ok(result.map(|r| r.subscriber_id))
}
