use std::fmt;
use std::error;
use std::sync::{Arc, Mutex};

use hyper::server::{Server, Request, Response};
use hyper::status::StatusCode;
use hyper::method::Method;
use hyper::header::ContentType;
use hyper::uri::RequestUri;
use serde_json;
use serde::Serialize;
use uuid::Uuid;
use chrono::{DateTime, UTC};

use super::config::Config;
use super::memdbstash::MemDbStash;
use super::{Result, Error, ResultExt, ErrorKind};

struct ServerContext {
    pub config: Config,
    pub stash: MemDbStash,
    cached_memdb_status: Mutex<Option<(DateTime<UTC>, HealthCheckResponse)>>,
}

pub struct ApiServer {
    ctx: Arc<ServerContext>,
}

pub struct ApiResponse {
    body: Vec<u8>,
    status: StatusCode,
}

#[derive(Debug)]
pub enum ApiError {
    NotFound,
    BadRequest,
    MethodNotAllowed,
    BadJson(Box<serde_json::Error>),
    InternalServerError(Box<Error>),
}

#[derive(Serialize)]
struct ApiErrorDescription {
    #[serde(rename="type")]
    pub ty: String,
    pub message: String,
}

#[derive(Serialize, Clone)]
struct HealthCheckResponse {
    pub is_healthy: bool,
    pub sync_lag: u32,
}

#[derive(Deserialize)]
struct SingleSymbol {
    addr: u64,
    image_addr: u64,
    image_vmaddr: Option<u64>,
    image_uuid: Option<Uuid>,
    image_path: Option<String>,
}

#[derive(Deserialize)]
struct SymbolLookupRequest {
    sdk_id: String,
    cpu_name: String,
    symbols: Vec<SingleSymbol>,
}

impl ApiError {
    pub fn get_status(&self) -> StatusCode {
        match *self {
            ApiError::NotFound => StatusCode::NotFound,
            ApiError::BadRequest => StatusCode::BadRequest,
            ApiError::MethodNotAllowed => StatusCode::MethodNotAllowed,
            ApiError::BadJson(_) => StatusCode::BadRequest,
            ApiError::InternalServerError(_) => StatusCode::InternalServerError,
        }
    }

    fn describe(&self) -> ApiErrorDescription {
        match *self {
            ApiError::NotFound => {
                ApiErrorDescription {
                    ty: "not_found".into(),
                    message: "The requested resource was not found".into(),
                }
            },
            ApiError::BadRequest => {
                ApiErrorDescription {
                    ty: "bad_request".into(),
                    message: "The client sent a bad request".into(),
                }
            },
            ApiError::MethodNotAllowed => {
                ApiErrorDescription {
                    ty: "method_not_allowed".into(),
                    message: "This HTTP method is not supported here".into(),
                }
            },
            ApiError::BadJson(ref json_err) => {
                ApiErrorDescription {
                    ty: "bad_json".into(),
                    message: format!("The client sent bad json: {}", json_err),
                }
            }
            ApiError::InternalServerError(ref err) => {
                ApiErrorDescription {
                    ty: "internal_server_error".into(),
                    message: format!(
                        "The server failed with an internal error: {}",
                        err),
                }
            }
        }
    }

    pub fn into_api_response(self) -> Result<ApiResponse> {
        ApiResponse::new(self.describe(), self.get_status())
    }
}

impl error::Error for ApiError {
    fn description(&self) -> &str {
        "API error"
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "API error")
    }
}

impl ServerContext {
    pub fn check_health(&self) -> Result<HealthCheckResponse> {
        let mut cache_value = self.cached_memdb_status.lock().unwrap();
        if_chain! {
            if let Some((ts, ref rv)) = *cache_value;
            if UTC::now() - self.config.get_server_healthcheck_ttl()? < ts;
            then { return Ok(rv.clone()); }
        }
        let state = self.stash.get_sync_status()?;
        let rv = HealthCheckResponse {
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

    pub fn from_error(err: Error) -> Result<ApiResponse> {
        if_chain! {
            if let &ErrorKind::ApiError(_) = err.kind();
            if let Error(ErrorKind::ApiError(api_error), _) = err;
            then {
                return api_error.into_api_response();
            }
        }

        // XXX: better logging here
        println!("INTERNAL SERVER ERROR: {}", &err);
        ApiResponse::new(ApiErrorDescription {
            ty: "internal_server_error".into(),
            message: format!("The server failed with an internal error: {}",
                &err),
        }, StatusCode::InternalServerError)
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
            let handler = match req.uri {
                RequestUri::AbsolutePath(ref path) => {
                    match path.as_str() {
                        "/health" => healthcheck_handler,
                        "/lookup" => lookup_symbol_handler,
                        _ => not_found,
                    }
                }
                _ => bad_request,
            };
            match handler(&*ctx.clone(), req) {
                Ok(result) => result,
                Err(err) => ApiResponse::from_error(err).unwrap(),
            }.write_to_response(resp).unwrap();
        })?;
        Ok(())
    }
}

fn healthcheck_handler(ctx: &ServerContext, req: Request) -> Result<ApiResponse>
{
    if req.method != Method::Get {
        return Err(ApiError::MethodNotAllowed.into());
    }
    let rv = ctx.check_health()?;
    let status = if rv.is_healthy {
        StatusCode::Ok
    } else {
        StatusCode::ServiceUnavailable
    };
    ApiResponse::new(rv, status)
}

fn lookup_symbol_handler(ctx: &ServerContext, mut req: Request) -> Result<ApiResponse>
{
    if req.method != Method::Post {
        return Err(ApiError::MethodNotAllowed.into());
    }
    let data : SymbolLookupRequest = match serde_json::from_reader(&mut req) {
        Ok(data) => data,
        Err(err) => { return Err(ApiError::BadJson(Box::new(err)).into()); }
    };
    Err(ApiError::NotFound.into())
}

fn bad_request(_: &ServerContext, _: Request) -> Result<ApiResponse>
{
    Err(ApiError::BadRequest.into())
}

fn not_found(_: &ServerContext, _: Request) -> Result<ApiResponse>
{
    Err(ApiError::NotFound.into())
}
