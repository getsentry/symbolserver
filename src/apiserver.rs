use std::sync::Arc;

use hyper::server::{Server, Request, Response};
use hyper::status::StatusCode;
use hyper::method::Method;
use hyper::uri::RequestUri;
use serde_json;
use serde::Serialize;

use super::config::Config;
use super::memdbstash::MemDbStash;
use super::{Result, ResultExt};

struct ServerContext {
    pub config: Config,
    pub stash: MemDbStash,
}

pub struct ApiServer {
    ctx: Arc<ServerContext>,
}

type Handler = fn(&ServerContext, Request, Response);

#[derive(Serialize)]
struct ApiError {
    #[serde(rename="type")]
    pub ty: String,
    pub message: String,
}

#[derive(Serialize)]
struct HealthCheckResult {
    is_healthy: bool,
    sync_lag: u32,
}

impl ApiServer {
    pub fn new(config: &Config) -> Result<ApiServer> {
        Ok(ApiServer {
            ctx: Arc::new(ServerContext {
                config: config.clone(),
                stash: MemDbStash::new(config)?,
            }),
        })
    }

    pub fn run(&self) -> Result<()> {
        let ctx = self.ctx.clone();
        Server::http(self.ctx.config.get_http_socket_addr()?)?
            .handle(move |mut req: Request, mut resp: Response|
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
            handler(&*ctx.clone(), req, resp).unwrap();
        })?;
        Ok(())
    }
}

fn respond<T: Serialize>(mut resp: Response, obj: T, status: StatusCode) -> Result<()> {
    *resp.status_mut() = status;
    let mut body : Vec<u8> = vec![];
    serde_json::to_writer(&mut body, &obj)
        .chain_err(|| "Failed to serialize response for client")?;
    resp.send(&body[..]);
    Ok(())
}

fn healthcheck_handler(ctx: &ServerContext, _: Request, mut resp: Response)
    -> Result<()>
{
    // TODO: cache this
    let state = ctx.stash.get_sync_status()?;
    respond(resp, HealthCheckResult {
        is_healthy: state.is_healthy(),
        sync_lag: state.lag(),
    }, if state.is_healthy() {
        StatusCode::Ok
    } else {
        StatusCode::ServiceUnavailable
    })
}

fn not_found_handler(_: &ServerContext, _: Request, resp: Response)
    -> Result<()>
{
    respond(resp, ApiError {
        ty: "not_found".into(),
        message: "The requested resource was not found".into()
    }, StatusCode::NotFound)
}

fn bad_request_handler(_: &ServerContext, _: Request, mut resp: Response)
    -> Result<()>
{
    respond(resp, ApiError {
        ty: "bad_request".into(),
        message: "The request could not be handled".into()
    }, StatusCode::BadRequest)
}

fn method_not_allowed_handler(_: &ServerContext, _: Request, mut resp: Response)
    -> Result<()>
{
    respond(resp, ApiError {
        ty: "method_not_allowed".into(),
        message: "The server cannot handle this method".into()
    }, StatusCode::MethodNotAllowed)
}
