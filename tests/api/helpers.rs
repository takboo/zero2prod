use once_cell::sync::Lazy;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use wiremock::MockServer;
use zero2prod::configuration::DatabaseSettings;
use zero2prod::email_client::SendEmailRequest;
use zero2prod::get_configuration;
use zero2prod::startup::{Application, get_connection_pool};
use zero2prod::telemetry::{get_subscriber, init_subscriber};

pub struct TestApp {
    pub connection_pool: PgPool,
    pub address: String,
    pub email_server: MockServer,
    pub port: u16,
}

pub struct ConfirmationLinks {
    pub html: reqwest::Url,
    pub plain_text: reqwest::Url,
}

impl TestApp {
    pub async fn post_subscriptions(&self, body: &'static str) -> reqwest::Response {
        let client = reqwest::Client::new();
        client
            .post(format!("{}/subscriptions", self.address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("Failed to execute request.")
    }
    pub async fn post_newsletters(&self, body: serde_json::Value) -> reqwest::Response {
        reqwest::Client::new()
            .post(format!("{}/newsletters", &self.address))
            .json(&body)
            .send()
            .await
            .expect("Failed to execute request.")
    }

    pub fn get_confirmation_links(&self, request: &wiremock::Request) -> ConfirmationLinks {
        let body: SendEmailRequest =
            serde_json::from_slice(&request.body).expect("Invalid email request body");
        let html = self.get_url_link(&body.html);
        let plain_text = self.get_url_link(&body.text);
        ConfirmationLinks { html, plain_text }
    }

    fn get_url_link(&self, s: &str) -> reqwest::Url {
        let links: Vec<_> = linkify::LinkFinder::new()
            .links(s)
            .filter(|l| *l.kind() == linkify::LinkKind::Url)
            .collect();
        let raw_link = links.get(0).expect("Failed to find raw url").as_str();
        let mut confirmation_link = reqwest::Url::parse(raw_link).expect("Invalid raw url");
        // Let's make sure we don't call random APIs on the web
        assert_eq!(confirmation_link.host_str().unwrap(), "127.0.0.1");
        confirmation_link.set_port(Some(self.port)).unwrap();
        confirmation_link
    }
}

static TRACING: Lazy<()> = Lazy::new(|| {
    let default_filter_level = "info".to_string();
    let subscriber_name = "test".to_string();

    if std::env::var("TEST_LOG").is_ok() {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::stdout);
        init_subscriber(subscriber);
    } else {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::sink);
        init_subscriber(subscriber);
    }
});

async fn spawn_app_impl(base_url_override: Option<String>) -> TestApp {
    Lazy::force(&TRACING);

    let email_server = MockServer::start().await;

    let configuration = {
        let mut c = get_configuration().expect("Failed to read configuration.");
        c.database.database_name = uuid::Uuid::new_v4().to_string();
        c.application.port = 0;
        c.email_client.base_url = email_server.uri();

        if let Some(base_url) = base_url_override {
            c.application.base_url = base_url;
        }
        c
    };

    configure_database(&configuration.database).await;
    let application = Application::build(configuration.clone())
        .await
        .expect("Failed to build application.");

    let application_port = application.port();
    let address = format!("http://127.0.0.1:{}", application_port);
    tokio::spawn(application.run_until_stopped());

    TestApp {
        address,
        email_server,
        connection_pool: get_connection_pool(&configuration.database),
        port: application_port,
    }
}

pub async fn spawn_app() -> TestApp {
    spawn_app_impl(None).await
}

pub async fn spawn_app_with_base_url(base_url: String) -> TestApp {
    spawn_app_impl(Some(base_url)).await
}

async fn configure_database(config: &DatabaseSettings) -> PgPool {
    //Create Database
    let mut connection = PgConnection::connect_with(&config.without_db())
        .await
        .expect("Failed to connect to Postgres");
    connection
        .execute(format!(r#"CREATE DATABASE "{}";"#, config.database_name).as_str())
        .await
        .expect("Failed to create database.");

    // create pgpool and migration
    let connection_pool = PgPool::connect_with(config.with_db())
        .await
        .expect("Failed to connect to Postgres");

    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to run database migrations.");

    connection_pool
}
