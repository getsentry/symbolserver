use std::sync::{Arc, Mutex};

use hyper::server::{Server, Request, Response};
use hyper::status::StatusCode;
use hyper::method::Method;
use hyper::header::ContentType;
use hyper::uri::RequestUri;
use serde_json;
use serde::Serialize;
use chrono::{DateTime, UTC};

use super::config::Config;
use super::memdbstash::MemDbStash;
use super::{Result, ResultExt};

struct ServerContext {
    pub config: Config,
    pub stash: MemDbStash,
    cached_memdb_status: Mutex<Option<(DateTime<UTC>, HealthCheckResult)>>,
}

pub struct ApiServer {
    ctx: Arc<ServerContext>,
}

pub struct ApiResponse {
    body: Vec<u8>,
    status: StatusCode,
}

#[derive(Serialize)]
struct ApiError {
    #[serde(rename="type")]
    pub ty: String,
    pub message: String,
}

#[derive(Serialize, Clone)]
struct HealthCheckResult {
    pub is_healthy: bool,
    pub sync_lag: u32,
}

impl ServerContext {
    pub fn check_health(&self) -> Result<HealthCheckResult> {
        let mut cache_value = self.cached_memdb_status.lock().unwrap();
        if_chain! {
            if let Some((ts, ref rv)) = *cache_value;
            if UTC::now() - self.config.get_server_healthcheck_ttl()? < ts;
            then {
                return Ok(rv.clone());
            }
        }
        let state = self.stash.get_sync_status()?;
        let rv = HealthCheckResult {
            is_healthy: state.is_healthy(),
            sync_lag: state.lag(),
        };
        *cache_value = Some((UTC::now(), rv.clone()));
        Ok(rv)
    }
}

impl ApiResponse {
    pub fn new<S: Serialize>(data: S, status: StatusCode) -> Result<ApiResponse> {
        let mut body : Vec<u8> = vec![];
        serde_json::to_writer(&mut body, &data)
            .chain_err(|| "Failed to serialize response for client")?;
        Ok(ApiResponse {
            body: body,
            status: status,
        })
    }

    pub fn write_to_response(&self, mut resp: Response) -> Result<()> {
        *resp.status_mut() = self.status;
        resp.headers_mut().set(ContentType::json());
        resp.send(&self.body[..])?;
        Ok(())
    }
}

impl ApiServer {
    pub fn new(config: &Config) -> Result<ApiServer> {
        Ok(ApiServer {
            ctx: Arc::new(ServerContext {
                config: config.clone(),
                stash: MemDbStash::new(config)?,
                cached_memdb_status: Mutex::new(None),
            }),
        })
    }

    pub fn run(&self) -> Result<()> {
        let ctx = self.ctx.clone();
        Server::http(self.ctx.config.get_server_socket_addr()?)?
            .handle(move |req: Request, resp: Response|
        {
            let handler = match req.method {
                Method::Get => {
                    if let RequestUri::AbsolutePath(ref path) = req.uri {
                        match path.as_str() {
                            "/health" => healthcheck_handler,
                            _ => not_found_handler,
                        }
                    } else {
                        bad_request_handler
                    }
                }
                _ => {
                    method_not_allowed_handler
                }
            };
            match handler(&*ctx.clone(), req) {
                Ok(result) => result,
                Err(err) => {
                    // XXX: better logging here
                    println!("INTERNAL SERVER ERROR: {}", &err);
                    ApiResponse::new(ApiError {
                        ty: "internal_server_error".into(),
                        message: format!("The server failed with an internal error: {}",
                            &err),
                    }, StatusCode::InternalServerError).unwrap()
                }
            }.write_to_response(resp).unwrap();
        })?;
        Ok(())
    }
}

fn healthcheck_handler(ctx: &ServerContext, _: Request) -> Result<ApiResponse>
{
    let rv = ctx.check_health()?;
    let status = if rv.is_healthy {
        StatusCode::Ok
    } else {
        StatusCode::ServiceUnavailable
    };
    ApiResponse::new(rv, status)
}

fn not_found_handler(_: &ServerContext, _: Request) -> Result<ApiResponse>
{
    ApiResponse::new(ApiError {
        ty: "not_found".into(),
        message: "The requested resource was not found".into()
    }, StatusCode::NotFound)
}

fn bad_request_handler(_: &ServerContext, _: Request) -> Result<ApiResponse>
{
    ApiResponse::new(ApiError {
        ty: "bad_request".into(),
        message: "The request could not be handled".into()
    }, StatusCode::BadRequest)
}

fn method_not_allowed_handler(_: &ServerContext, _: Request) -> Result<ApiResponse>
{
    ApiResponse::new(ApiError {
        ty: "method_not_allowed".into(),
        message: "The server cannot handle this method".into()
    }, StatusCode::MethodNotAllowed)
}
