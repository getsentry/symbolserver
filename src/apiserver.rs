use std::sync::Arc;

use iron::{Iron, IronResult, Request, Response, Chain, Handler};
use iron::status::Status;
use router::Router;
use persistent;

use super::config::Config;
use super::memdbstash::MemDbStash;
use super::Result;


struct ServerContext {
    config: Config,
    stash: MemDbStash,
}

pub struct ApiServer {
    ctx: ServerContext,
}


impl ApiServer {
    pub fn new(config: &Config) -> Result<ApiServer> {
        Ok(ApiServer {
            ctx: ServerContext {
                config: config.clone(),
                stash: MemDbStash::new(config)?,
            },
        })
    }

    pub fn make_handler(&self) -> Result<Box<Handler>> {
        let mut router = Router::new();

        router.get("/health", HealthCheck, "healthcheck");

        let chain = Chain::new(router);

        Ok(Box::new(chain))
    }

    pub fn run(&self) -> Result<()> {
        let app = Iron::new(self.make_handler()?);
        app.http(self.ctx.config.get_http_socket_addr()?)?;
        Ok(())
    }
}

struct HealthCheck;

impl Handler for HealthCheck {
    fn handle(&self, _: &mut Request) -> IronResult<Response> {
        Ok(Response::with((Status::Ok, "Stuff\n")))
    }
}
