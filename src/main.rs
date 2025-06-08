use sqlx::postgres::PgPoolOptions;
use std::net::TcpListener;
use zero2prod::startup::run;
use zero2prod::{
    get_configuration,
    telemetry::{get_subscriber, init_subscriber},
};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let subscriber = get_subscriber("zero2prod".into(), "info".into(), std::io::stdout);
    init_subscriber(subscriber);

    let configuration = get_configuration().expect("Failed to read configuration.");
    let pg_pool = PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_secs(
            configuration.database.acquire_timeout_secs,
        ))
        .connect_lazy_with(configuration.database.with_db());

    let address = format!(
        "{}:{}",
        configuration.application.host, configuration.application.port
    );
    let listener = TcpListener::bind(address).expect("Failed to bind port 8080");
    run(listener, pg_pool)?.await
}
