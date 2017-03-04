use hyper::server::Request;
use hyper::status::StatusCode;
use hyper::method::Method;
use uuid::Uuid;

use super::super::Result;
use super::super::utils::Addr;
use super::super::memdb::Symbol as MemDbSymbol;
use super::server::{ServerContext, load_request_data};
use super::types::{ApiResponse, ApiError};

#[derive(Deserialize)]
struct SymbolLookupRequest {
    sdk_id: String,
    cpu_name: String,
    symbols: Vec<Symbol>,
}

#[derive(Serialize, Deserialize)]
struct Symbol {
    object_uuid: Option<Uuid>,
    object_name: Option<String>,
    symbol: Option<String>,
    addr: Addr,
}

impl<'a> From<MemDbSymbol<'a>> for Symbol {
    fn from(sym: MemDbSymbol<'a>) -> Symbol {
        Symbol {
            object_uuid: Some(sym.object_uuid()),
            object_name: Some(sym.object_name().to_string()),
            symbol: Some(sym.symbol().to_string()),
            addr: Addr(sym.addr()),
        }
    }
}

#[derive(Serialize)]
struct SymbolResponse {
    symbols: Vec<Option<Symbol>>,
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

    let data: SymbolLookupRequest = load_request_data(&mut req)?;
    let sdk_infos = ctx.stash.fuzzy_match_sdk_id(&data.sdk_id)?;
    if sdk_infos.is_empty() {
        return Err(ApiError::SdkNotFound.into());
    }

    let mut rv = vec![];
    for symq in data.symbols {
        let mut rvsym = None;
        if let Some(ref uuid) = symq.object_uuid {
            for sdk_info in sdk_infos.iter() {
                let sdk = ctx.stash.get_memdb(&sdk_info)?;
                if let Some(sym) = sdk.lookup_by_uuid(uuid, symq.addr.into()) {
                    rvsym = Some(sym.into());
                    break;
                }
            }
        } else if let Some(ref name) = symq.object_name {
            for sdk_info in sdk_infos.iter() {
                let sdk = ctx.stash.get_memdb(&sdk_info)?;
                if let Some(sym) = sdk.lookup_by_object_name(
                   name, &data.cpu_name, symq.addr.into()) {
                    rvsym = Some(sym.into());
                    break;
                }
            }
        }
        rv.push(rvsym);
    }

    ApiResponse::new(SymbolResponse {
        symbols: rv,
    }, StatusCode::Ok)
}
