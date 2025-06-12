use crate::EmailClient;
use crate::configuration::{DatabaseSettings, Settings};
use crate::routes::{confirm, health_check, subscribe};
use actix_web::dev::Server;
use actix_web::{App, HttpServer, web::Data};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

pub struct Application {
    port: u16,
    server: Server,
}

impl Application {
    pub async fn build(configuration: Settings) -> Result<Self, std::io::Error> {
        let pg_pool = get_connection_pool(&configuration.database);

        let email_client = EmailClient::new(
            configuration.email_client.base_url,
            configuration.email_client.sender_email,
            configuration.email_client.authorization_token,
            configuration.email_client.timeout,
        );

        let address = format!(
            "{}:{}",
            configuration.application.host, configuration.application.port
        );
        let listener = TcpListener::bind(address).expect("Failed to bind port 8080");
        let port = listener.local_addr()?.port();
        let server = run(
            listener,
            pg_pool,
            email_client,
            ApplicationBaseUrl(configuration.application.base_url),
        )?;

        Ok(Self { port, server })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn run_until_stopped(self) -> Result<(), std::io::Error> {
        self.server.await
    }
}

pub fn get_connection_pool(db_configuration: &DatabaseSettings) -> PgPool {
    PgPoolOptions::new()
        .acquire_timeout(db_configuration.acquire_timeout)
        .connect_lazy_with(db_configuration.with_db())
}

pub struct ApplicationBaseUrl(pub String);

fn run(
    listener: TcpListener,
    pg_pool: PgPool,
    email_client: EmailClient,
    base_url: ApplicationBaseUrl,
) -> Result<Server, std::io::Error> {
    let pg_pool = Data::new(pg_pool);
    let email_client = Data::new(email_client);
    let base_url = Data::new(base_url);

    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .app_data(pg_pool.clone())
            .app_data(email_client.clone())
            .app_data(base_url.clone())
            .service(health_check)
            .service(subscribe)
            .service(confirm)
    })
    .listen(listener)?
    .run();
    Ok(server)
}
