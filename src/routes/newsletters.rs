use crate::EmailClient;
use crate::domain::SubscriberEmail;
use crate::routes::error_chain_fmt;
use crate::telemetry::spawn_blocking_with_tracing;
use actix_web::dev::Payload;
use actix_web::http::header::HeaderValue;
use actix_web::http::{StatusCode, header};
use actix_web::{FromRequest, HttpRequest, HttpResponse, ResponseError, post, web};
use anyhow::Context;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use secrecy::{ExposeSecret, SecretString};
use sqlx::PgPool;
use std::future::{Ready, ready};

#[derive(serde::Deserialize)]
pub struct BodyData {
    title: String,
    content: Content,
}

#[derive(serde::Deserialize)]
pub struct Content {
    html: String,
    text: String,
}

#[derive(thiserror::Error)]
pub enum PublishError {
    #[error("Authentication failed")]
    AuthError(#[source] anyhow::Error),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for PublishError {
    fn error_response(&self) -> HttpResponse {
        match self {
            PublishError::UnexpectedError(_) => {
                HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR)
            }
            PublishError::AuthError(_) => {
                let mut response = HttpResponse::new(StatusCode::UNAUTHORIZED);
                let header_value = HeaderValue::from_static(r#"Basic realm="publish""#);
                response
                    .headers_mut()
                    .insert(header::WWW_AUTHENTICATE, header_value);
                response
            }
        }
    }
}

#[derive(Debug)]
struct BasicAuthorization {
    username: String,
    password: SecretString,
}

impl FromRequest for BasicAuthorization {
    type Error = PublishError;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let auth_header = match req
            .headers()
            .get(header::AUTHORIZATION)
            .context("The 'Authorization' header was missing")
        {
            Ok(header) => header,
            Err(e) => return ready(Err(PublishError::AuthError(e))),
        };

        let auth_str = match auth_header
            .to_str()
            .context("The 'Authorization' header was not a valid UTF8 string")
        {
            Ok(s) => s,
            Err(e) => return ready(Err(PublishError::AuthError(e))),
        };

        let base64encoded_segment = match auth_str
            .strip_prefix("Basic ")
            .context("The authorization scheme was not 'Basic'")
        {
            Ok(s) => s,
            Err(e) => return ready(Err(PublishError::AuthError(e))),
        };

        let decoded_bytes = match BASE64_STANDARD
            .decode(base64encoded_segment)
            .context("Failed to base64-decode 'Basic' credentials")
        {
            Ok(b) => b,
            Err(e) => return ready(Err(PublishError::AuthError(e))),
        };
        let decoded_credentials = match String::from_utf8(decoded_bytes)
            .context("The decoded credential string is not valid UTF8")
        {
            Ok(s) => s,
            Err(e) => return ready(Err(PublishError::AuthError(e))),
        };

        let mut credentials = decoded_credentials.splitn(2, ":");
        let username = match credentials
            .next()
            .context("A username must be provided in 'Basic' auth")
        {
            Ok(s) => s,
            Err(e) => return ready(Err(PublishError::AuthError(e))),
        }
        .to_string();

        let password = match credentials
            .next()
            .context("A password must be provided in 'Basic' auth")
        {
            Ok(s) => s,

            Err(e) => return ready(Err(PublishError::AuthError(e))),
        }
        .to_string();

        let password = SecretString::from(password);
        ready(Ok(BasicAuthorization { username, password }))
    }
}

#[tracing::instrument(
    name = "publish a newsletters to all confirmed subscribes",
    skip(pg_pool, body, email_client, auth)
    fields(username=auth.username, user_id=tracing::field::Empty)
)]
#[post("newsletters")]
async fn publish_newsletter(
    pg_pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    body: web::Json<BodyData>,
    auth: BasicAuthorization,
) -> Result<HttpResponse, PublishError> {
    let user_id = validate_credentials(auth, &pg_pool).await?;
    tracing::Span::current().record("user_id", tracing::field::display(&user_id));

    let subscribers = get_confirmed_subscribers(&pg_pool)
        .await
        .context("Failed to get all confirmed subscribers")?;
    for subscriber in subscribers {
        match subscriber {
            Ok(subscriber) => {
                email_client
                    .send_email(
                        &subscriber.email,
                        &body.title,
                        &body.content.html,
                        &body.content.text,
                    )
                    .await
                    .with_context(|| {
                        format!("Failed to send newsletter issue to {}", subscriber.email)
                    })?;
            }
            Err(e) => {
                tracing::warn!(
                    // We record the error chain as a structured field
                    // on the log record.
                    error.cause_chain = ?e,
                    "Skipping a confirmed subscriber. Their stored contact details are invalid",
                );
            }
        }
    }
    Ok(HttpResponse::Ok().finish())
}

struct ConfirmedSubscriber {
    email: SubscriberEmail,
}

#[tracing::instrument(name = "Get confirmed subscribers", skip(pg_pool))]
async fn get_confirmed_subscribers(
    pg_pool: &PgPool,
) -> Result<Vec<Result<ConfirmedSubscriber, anyhow::Error>>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        SELECT email
        FROM subscriptions
        WHERE status = 'confirmed'
        "#,
    )
    .fetch_all(pg_pool)
    .await?
    .into_iter()
    .map(|r| match r.email.try_into() {
        Ok(email) => Ok(ConfirmedSubscriber { email }),
        Err(e) => Err(anyhow::anyhow!(e)),
    })
    .collect();
    Ok(rows)
}

#[tracing::instrument(name = "Get stored credentials", skip(username, pg_pool))]
async fn get_stored_credentials(
    username: &str,
    pg_pool: &PgPool,
) -> Result<Option<(uuid::Uuid, SecretString)>, anyhow::Error> {
    let row: Option<_> = sqlx::query!(
        r#"
        SELECT user_id, password_hash
        FROM users
        WHERE username = $1
        "#,
        username,
    )
    .fetch_optional(pg_pool)
    .await
    .context("Failed to perform a query to validate auth credentials")?
    .map(|r| (r.user_id, SecretString::from(r.password_hash)));
    Ok(row)
}

#[tracing::instrument(name = "Validate credentials", skip(credentials, pg_pool))]
async fn validate_credentials(
    credentials: BasicAuthorization,
    pg_pool: &PgPool,
) -> Result<uuid::Uuid, PublishError> {
    let mut user_id = None;
    let mut expected_password_hash = SecretString::from(
        "$argon2id$v=19$m=15000,t=2,p=1$\
        gZiV/M1gPc22ElAH/Jh1Hw$\
        CWOrkoo7oJBQ/iyh7uJ0LO2aLEfrHwTWllSAxT0zRno",
    );

    if let Some((stored_user_id, stored_password_hash)) =
        get_stored_credentials(&credentials.username, pg_pool)
            .await
            .map_err(PublishError::UnexpectedError)?
    {
        user_id = Some(stored_user_id);
        expected_password_hash = stored_password_hash;
    }

    spawn_blocking_with_tracing(move || {
        verify_password_hash(expected_password_hash, credentials.password)
    })
    .await
    .context("Failed to spawn blocking task.")
    .map_err(PublishError::UnexpectedError)??;

    user_id.ok_or_else(|| PublishError::AuthError(anyhow::anyhow!("Unknown username.")))
}
#[tracing::instrument(
    name = "Verify password hash",
    skip(expected_password_hash, password_candidate)
)]
fn verify_password_hash(
    expected_password_hash: SecretString,
    password_candidate: SecretString,
) -> Result<(), PublishError> {
    let expected_password_hash = PasswordHash::new(expected_password_hash.expose_secret())
        .context("Failed to parse hash in PHC string format.")
        .map_err(PublishError::UnexpectedError)?;

    Argon2::default()
        .verify_password(
            password_candidate.expose_secret().as_bytes(),
            &expected_password_hash,
        )
        .context("Invalid password.")
        .map_err(PublishError::AuthError)
}
