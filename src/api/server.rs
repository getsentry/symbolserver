//! Implements the API server.
use std::sync::{Arc, RwLock};
use std::thread;
use std::io::Read;
use std::os::unix::io::{FromRawFd, RawFd};

use libc;
use hyper::server::{Server, Request, Response};
use hyper::header::ContentLength;
use hyper::method::Method;
use hyper::net::HttpListener;
use hyper::uri::RequestUri;
use serde::Deserialize;
use serde_json;

use super::super::config::Config;
use super::super::memdb::stash::{MemDbStash, SyncStatus};
use super::super::Result;
use super::super::utils::{HumanDuration, run_isolated, get_systemd_fd};
use super::handlers;
use super::types::{ApiResponse, ApiError};

/// Result from a healthcheck.
#[derive(Serialize, Clone)]
pub struct HealthCheckResponse {
    pub is_offline: bool,
    pub is_healthy: bool,
    pub sync_lag: u32,
}

/// Shared access to the state of the server.
pub struct ServerContext {
    pub config: Config,
    pub stash: MemDbStash,
    enable_sync: bool,
    cached_memdb_status: RwLock<Option<SyncStatus>>,
}

/// The API server itself.
pub struct ApiServer {
    ctx: Arc<ServerContext>,
}

/// Controls how the server binds to sockets.
pub enum BindOptions<'a> {
    /// Bind according to the config file.
    UseConfig,
    /// Bind to a file descriptor.
    BindToFd(RawFd),
    /// Bind to a specific address (`host:port`).
    BindToAddr(&'a str),
}

impl ServerContext {
    pub fn check_health(&self) -> Result<()> {
        let sync_status = self.stash.get_sync_status()?;
        *self.cached_memdb_status.write().unwrap() = Some(sync_status);
        Ok(())
    }

    pub fn get_healthcheck_result(&self) -> Result<HealthCheckResponse> {
        if self.enable_sync {
            let cache_value = self.cached_memdb_status.read().unwrap();
            if let Some(ref state) = *cache_value {
                Ok(HealthCheckResponse {
                    is_offline: state.is_offline(),
                    is_healthy: state.is_healthy(),
                    sync_lag: state.lag()
                })
            } else {
                Ok(HealthCheckResponse {
                    is_offline: true,
                    is_healthy: false,
                    sync_lag: 0,
                })
            }
        } else {
            Ok(HealthCheckResponse {
                is_offline: true,
                is_healthy: true,
                sync_lag: 0,
            })
        }
    }
}

impl ApiServer {
    /// Create a new server.
    pub fn new(config: &Config, enable_sync: bool) -> Result<ApiServer> {
        Ok(ApiServer {
            ctx: Arc::new(ServerContext {
                config: config.clone(),
                stash: MemDbStash::new(config)?,
                enable_sync: enable_sync,
                cached_memdb_status: RwLock::new(None),
            }),
        })
    }

    /// Spawns a background thread that runs the sync process.
    pub fn spawn_sync_thread(&self) -> Result<()> {
        let interval = self.ctx.config.get_server_sync_interval()?;
        let std_interval = interval.to_std().unwrap();
        info!("Checking for symbols from S3 in background every {}",
              HumanDuration(interval));
        info!("Source Bucket: {}", self.ctx.config.get_aws_bucket_url()?);
        info!("Local SDKs: {}", self.ctx.stash.sdk_count()?);

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

    /// Spawns a background check that checks the health of the system.
    pub fn spawn_healthcheck_thread(&self) -> Result<()> {
        let interval = self.ctx.config.get_server_healthcheck_interval()?;
        let std_interval = interval.to_std().unwrap();
        info!("Running healthcheck every {}", HumanDuration(interval));

        // run initial healthcheck right away.
        let ctx = self.ctx.clone();
        ctx.check_health()?;

        thread::spawn(move || {
            loop {
                let ctx = ctx.clone();
                run_isolated(move || ctx.check_health());
                thread::sleep(std_interval);
            }
        });

        Ok(())
    }

    /// Runs the server in a loop.
    pub fn run(&self, threads: usize, opts: BindOptions) -> Result<()> {
        let debug_addr;

        if self.ctx.enable_sync {
            self.spawn_sync_thread()?;
            self.spawn_healthcheck_thread()?;
        } else {
            info!("Background sync is disabled. Health check forced to healthy.");
        }

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
                if let Some(fd) = get_systemd_fd()? {
                    debug_addr = format!("systemd supplied fd");
                    // unsafe is sortof okay here because get_systemd_fd will
                    // not return the fd a second time and we also do not pass
                    // that information to potential children
                    unsafe { HttpListener::from_raw_fd(fd) }
                } else {
                    let addr = self.ctx.config.get_server_socket_addr()?;
                    let (host, port) = addr;
                    debug_addr = format!("http://{}:{}/", host, port);
                    HttpListener::new((host.as_str(), port))?
                }
            }
        };
        info!("Listening on {}", debug_addr);
        info!("Spawning {} listener threads", threads);

        let ctx = self.ctx.clone();
        Server::new(listener)
            .handle_threads(move |req: Request, resp: Response|
        {
            let is_head = req.method == Method::Head;
            let handler = match req.uri {
                RequestUri::AbsolutePath(ref path) => {
                    match path.as_str() {
                        "/health" => handlers::healthcheck_handler,
                        "/lookup" => handlers::lookup_symbol_handler,
                        "/sdks" => handlers::list_sdks_handler,
                        _ => not_found,
                    }
                }
                _ => bad_request,
            };
            match handler(&*ctx.clone(), req) {
                Ok(result) => result,
                Err(err) => ApiResponse::from_error(err).unwrap(),
            }.write_to_response(is_head, resp).unwrap();
        }, threads)?;
        Ok(())
    }
}

/// Helper for the handlers to safely load request data.
pub fn load_request_data<D: Deserialize>(req: &mut Request) -> Result<D> {
    if let Some(&ContentLength(length)) = req.headers.get() {
        if length > 1024 * 1024 * 2 {
            return Err(ApiError::PayloadTooLarge.into());
        }
    } else {
        return Err(ApiError::BadRequest.into());
    }

    let mut body: Vec<u8> = vec![];
    req.read_to_end(&mut body)?;

    Ok(match serde_json::from_slice(&body[..]) {
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
