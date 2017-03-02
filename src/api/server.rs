use std::sync::{Arc, Mutex};
use std::thread;
use std::os::unix::io::{FromRawFd, RawFd};

use libc;
use hyper::server::{Server, Request, Response};
use hyper::net::HttpListener;
use hyper::uri::RequestUri;
use chrono::{DateTime, UTC};
use serde::Deserialize;
use serde_json;

use super::super::config::Config;
use super::super::memdbstash::MemDbStash;
use super::super::Result;
use super::super::utils::{HumanDuration, run_isolated};
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

pub enum BindOptions<'a> {
    UseConfig,
    BindToFd(RawFd),
    BindToAddr(&'a str),
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

    fn spawn_sync_thread(&self) -> Result<()> {
        let interval = self.ctx.config.get_server_sync_interval()?;
        let std_interval = interval.to_std().unwrap();
        info!("Checking for symbols from S3 in background every {}",
              HumanDuration(interval));

        let ctx = self.ctx.clone();
        thread::spawn(move || {
            loop {
                let ctx = ctx.clone();
                run_isolated(move || ctx.stash.sync(Default::default()));
                thread::sleep(std_interval);
            }
        });

        Ok(())
    }

    pub fn run(&self, opts: BindOptions) -> Result<()> {
        let debug_addr;
        let listener = match opts {
            BindOptions::BindToAddr(addr) => {
                debug_addr = format!("http://{}/", addr);
                HttpListener::new(addr)?
            }
            BindOptions::BindToFd(fd) => {
                debug_addr = format!("file descriptor {}", fd);
                // unsafe is okay here because we dup the fd
                unsafe { HttpListener::from_raw_fd(libc::dup(fd)) }
            }
            BindOptions::UseConfig => {
                let addr = self.ctx.config.get_server_socket_addr()?;
                let (host, port) = addr;
                debug_addr = format!("http://{}:{}/", host, port);
                HttpListener::new((host, port))?
            }
        };
        info!("Listening on {}", debug_addr);
        self.spawn_sync_thread()?;

        let ctx = self.ctx.clone();
        Server::new(listener)
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
