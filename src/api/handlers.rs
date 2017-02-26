use hyper::server::Request;
use hyper::status::StatusCode;
use hyper::method::Method;
use uuid::Uuid;

use super::super::Result;
use super::super::sdk::SdkInfo;
use super::server::{ServerContext, load_request_data};
use super::types::{ApiResponse, ApiError};

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

pub fn healthcheck_handler(ctx: &ServerContext, req: Request) -> Result<ApiResponse>
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

pub fn lookup_symbol_handler(ctx: &ServerContext, mut req: Request) -> Result<ApiResponse>
{
    if req.method != Method::Post {
        return Err(ApiError::MethodNotAllowed.into());
    }
    let data : SymbolLookupRequest = load_request_data(&mut req)?;

    Err(ApiError::NotFound.into())
}
