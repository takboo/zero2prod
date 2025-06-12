use crate::helpers::{spawn_app, spawn_app_with_base_url};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn subscribe_returns_a_200_for_valid_form_data() {
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
}
#[tokio::test]
async fn subscribe_persists_the_new_subscriber() {
    // Arrange
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/api/send"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_server)
        .await;

    // Act
    app.post_subscriptions(body).await;

    // Assert
    let saved = sqlx::query!("SELECT email, name, status FROM subscriptions",)
        .fetch_one(&app.connection_pool)
        .await
        .expect("Failed to fetch saved subscription.");

    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
    assert_eq!(saved.status, "pending_confirmation");
}

#[tokio::test]
async fn subscribe_returns_a_500_when_the_subscription_fails() {
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

    // subscribe the same name and email will return a 500
    let response = app.post_subscriptions(body).await;

    assert_eq!(500, response.status().as_u16());
}

#[tokio::test]
async fn subscribe_returns_a_400_when_data_is_missing() {
    let app = spawn_app().await;

    let test_cases = vec![
        ("name=le%20guin", "missing the email"),
        ("email=ursula_le_guin%40gmail.com", "missing the name"),
        ("", "missing both name and email"),
    ];

    for (invalid_body, error_message) in test_cases {
        let response = app.post_subscriptions(invalid_body).await;

        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not fail with 400 Bad Request when the payload was {}",
            error_message
        );
    }
}

#[tokio::test]
async fn subscribe_returns_a_400_when_fields_are_present_but_invalid() {
    // Arrange
    let app = spawn_app().await;
    let test_cases = vec![
        ("name=&email=ursula_le_guin%40gmail.com", "empty name"),
        ("name=Ursula&email=", "empty email"),
        ("name=Ursula&email=definitely-not-an-email", "invalid email"),
    ];

    for (body, description) in test_cases {
        // Act
        let response = app.post_subscriptions(body).await;

        // Assert
        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not return a 400 Bad Request when the payload was {}.",
            description
        );
    }
}

#[tokio::test]
async fn subscribe_sends_a_confirmation_email_for_valid_data() {
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
    // The two links should be identical
    assert_eq!(confirmation_links.html, confirmation_links.plain_text);
}

#[tokio::test]
async fn subscribe_sends_a_confirmation_email_with_a_link_handling_base_url_variations() {
    // Arrange
    let base_urls = [
        "http://127.0.0.1".to_string(),
        "http://127.0.0.1/".to_string(),
    ];

    for base_url in base_urls {
        let app = spawn_app_with_base_url(base_url).await;
        let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

        Mock::given(path("/email"))
            .and(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&app.email_server)
            .await;

        // Act
        app.post_subscriptions(body).await;

        // Assert
        let email_request = &app.email_server.received_requests().await.unwrap()[0];
        let confirmation_links = app.get_confirmation_links(email_request);

        // The link returned by the app should have the correct base URL structure
        // regardless of the trailing slash in the configuration.
        // `get_confirmation_links` already adjusts the port for us.
        let mut expected_link_origin = reqwest::Url::parse(&app.address).unwrap();
        expected_link_origin.set_path("/subscriptions/confirm");

        assert_eq!(
            confirmation_links.html.origin(),
            expected_link_origin.origin()
        );
        assert_eq!(confirmation_links.html.path(), expected_link_origin.path());
    }
}
