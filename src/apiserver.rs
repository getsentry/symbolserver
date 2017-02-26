use std::sync::Arc;

use iron::{Iron, IronResult, Request, Response, Chain, Handler};
use iron::status::Status;
use router::Router;
use persistent;

use super::config::Config;
use super::memdbstash::MemDbStash;
use super::Result;


struct ServerContext<'a> {
    config: &'a Config,
    stash: MemDbStash<'a>,
}

pub struct ApiServer<'a> {
    ctx: ServerContext<'a>,
}


impl<'a> ApiServer<'a> {
    pub fn new(config: &'a Config) -> Result<ApiServer<'a>> {
        Ok(ApiServer {
            ctx: ServerContext {
                config: config,
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
