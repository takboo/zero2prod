use crate::EmailClient;
use crate::routes::{health_check, subscribe};
use actix_web::dev::Server;
use actix_web::{App, HttpServer, web::Data};
use sqlx::PgPool;
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

pub fn run(
    listener: TcpListener,
    pg_pool: PgPool,
    email_client: EmailClient,
) -> Result<Server, std::io::Error> {
    let pg_pool = Data::new(pg_pool);
    let email_client = Data::new(email_client);
    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .app_data(pg_pool.clone())
            .app_data(email_client.clone())
            .service(health_check)
            .service(subscribe)
    })
    .listen(listener)?
    .run();
    Ok(server)
}
