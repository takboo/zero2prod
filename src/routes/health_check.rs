use actix_web::{HttpResponse, Responder, get};

#[get("/health_check")]
async fn health_check() -> impl Responder {
    HttpResponse::Ok().finish()
}
