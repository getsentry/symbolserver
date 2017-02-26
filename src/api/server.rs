use std::sync::{Arc, Mutex};

use hyper::server::{Server, Request, Response};
use hyper::uri::RequestUri;
use chrono::{DateTime, UTC};
use serde::Deserialize;
use serde_json;

use super::super::config::Config;
use super::super::memdbstash::MemDbStash;
use super::super::Result;
use super::handlers::{healthcheck_handler, lookup_symbol_handler};
use super::types::{ApiResponse, ApiError};

#[derive(Serialize, Clone)]
pub struct HealthCheckResponse {
    pub is_healthy: bool,
    pub sync_lag: u32,
}

pub struct ServerContext {
    pub config: Config,
    pub stash: MemDbStash,
    cached_memdb_status: Mutex<Option<(DateTime<UTC>, HealthCheckResponse)>>,
}

pub struct ApiServer {
    ctx: Arc<ServerContext>,
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

pub fn load_request_data<D: Deserialize>(req: &mut Request) -> Result<D> {
    Ok(match serde_json::from_reader(req) {
        Ok(data) => data,
        Err(err) => { return Err(ApiError::BadJson(Box::new(err)).into()); }
    })
}

fn bad_request(_: &ServerContext, _: Request) -> Result<ApiResponse>
{
    Err(ApiError::BadRequest.into())
}

fn not_found(_: &ServerContext, _: Request) -> Result<ApiResponse>
{
    Err(ApiError::NotFound.into())
}
