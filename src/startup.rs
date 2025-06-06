use crate::routes::{health_check, subscribe};
use actix_web::dev::Server;
use actix_web::{web, App, HttpServer};
use sqlx::PgPool;
use std::net::TcpListener;

pub fn run(listener: TcpListener, pg_pool: PgPool) -> Result<Server, std::io::Error> {
    let pg_pool = web::Data::new(pg_pool);
    let server = HttpServer::new(move || {
        App::new()
            .service(health_check)
            .service(subscribe)
            .app_data(pg_pool.clone())
    })
    .listen(listener)?
    .run();
    Ok(server)
}
