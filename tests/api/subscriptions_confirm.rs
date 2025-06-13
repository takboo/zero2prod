use crate::helpers::spawn_app;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn confirmations_without_token_are_rejected_with_a_400() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let response = reqwest::get(&format!("{}/subscriptions/confirm", app.address))
        .await
        .unwrap();

    // Assert
    assert_eq!(response.status().as_u16(), 400);
}

#[tokio::test]
async fn the_link_returned_by_subscribe_returns_a_200_if_called() {
    // Arrange
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/api/send"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    app.post_subscriptions(body).await;
    let email_request = &app
        .email_server
        .received_requests()
        .await
        .expect("No email request received")[0];
    let confirmation_links = app.get_confirmation_links(&email_request);

    // Act
    let response = reqwest::get(confirmation_links.html)
        .await
        .expect("Failed to confirm subscription");

    // Assert
    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn clicking_on_the_confirmation_link_confirms_a_subscriber() {
    let app = spawn_app().await;

    Mock::given(path("/api/send"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";
    let response = app.post_subscriptions(body).await;
    assert_eq!(200, response.status().as_u16());

    // Assert
    // Get the first intercepted request
    let email_request = &app
        .email_server
        .received_requests()
        .await
        .expect("missing email request")[0];
    // Parse the body as JSON, starting from raw bytes
    let confirmation_links = app.get_confirmation_links(email_request);
    // Act
    reqwest::get(confirmation_links.html)
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    // Assert
    let saved = sqlx::query!("SELECT email, name, status FROM subscriptions",)
        .fetch_one(&app.connection_pool)
        .await
        .expect("Failed to fetch saved subscription.");

    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
    assert_eq!(saved.status, "confirmed");
}

#[tokio::test]
async fn confirmations_for_a_non_existing_token_are_rejected_with_a_401() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let response = reqwest::get(&format!(
        "{}/subscriptions/confirm?subscription_token=abcdef",
        app.address
    ))
    .await
    .unwrap();

    // Assert
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn query_fails_if_the_database_is_corrupted_on_token_lookup() {
    let app = spawn_app().await;

    // Sabotage the database
    sqlx::query!("ALTER TABLE subscription_tokens DROP COLUMN subscription_token;",)
        .execute(&app.connection_pool)
        .await
        .unwrap();

    // Act
    let response = reqwest::get(&format!(
        "{}/subscriptions/confirm?subscription_token=abcdef",
        app.address
    ))
    .await
    .unwrap();

    // Assert
    assert_eq!(response.status().as_u16(), 500);
}

#[tokio::test]
async fn query_fails_if_the_database_is_corrupted_on_status_update() {
    // Arrange
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/api/send"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    app.post_subscriptions(body).await;
    let email_request = &app
        .email_server
        .received_requests()
        .await
        .expect("No email request received")[0];
    let confirmation_links = app.get_confirmation_links(&email_request);

    // Sabotage the database
    sqlx::query!("ALTER TABLE subscriptions DROP COLUMN status;",)
        .execute(&app.connection_pool)
        .await
        .unwrap();

    // Act
    let response = reqwest::get(confirmation_links.html).await.unwrap();

    // Assert
    assert_eq!(response.status().as_u16(), 500);
}
