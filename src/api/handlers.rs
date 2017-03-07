//! The handlers for the API endpoints.
use std::sync::Arc;
use std::collections::HashMap;

use hyper::server::Request;
use hyper::status::StatusCode;
use hyper::method::Method;
use uuid::Uuid;

use super::super::Result;
use super::super::utils::Addr;
use super::super::sdk::SdkInfo;
use super::super::memdb::read::{MemDb, Symbol as MemDbSymbol};
use super::super::memdb::stash::MemDbStash;
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

#[derive(Serialize)]
struct SdksResponse {
    sdks: Vec<String>,
}

struct LocalMemDbCache<'a> {
    stash: &'a MemDbStash,
    cache: HashMap<SdkInfo, Arc<MemDb<'static>>>,
}

impl<'a> LocalMemDbCache<'a> {
    pub fn new(stash: &'a MemDbStash) -> LocalMemDbCache<'a> {
        LocalMemDbCache {
            stash: stash,
            cache: HashMap::new(),
        }
    }

    pub fn get_memdb(&mut self, info: &SdkInfo) -> Result<Arc<MemDb<'static>>> {
        if let Some(memdb) = self.cache.get(&info) {
            return Ok(memdb.clone());
        }
        let rv = self.stash.get_memdb(info)?;
        self.cache.insert(info.clone(), rv.clone());
        Ok(rv)
    }
}

/// Implements the health check.
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

/// Implements the system symbol lookup.
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

    let mut lc = LocalMemDbCache::new(&ctx.stash);

    let mut rv = vec![];
    for symq in data.symbols {
        let mut rvsym = None;
        if let Some(ref uuid) = symq.object_uuid {
            for sdk_info in sdk_infos.iter() {
                if let Some(sym) = lc.get_memdb(sdk_info)?.lookup_by_uuid(
                   uuid, symq.addr.into()) {
                    rvsym = Some(sym.into());
                    break;
                }
            }
        } else if let Some(ref name) = symq.object_name {
            for sdk_info in sdk_infos.iter() {
                if let Some(sym) = lc.get_memdb(sdk_info)?.lookup_by_object_name(
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

/// Lists all found SDKs.
pub fn list_sdks_handler(ctx: &ServerContext, req: Request) -> Result<ApiResponse>
{
    if req.method != Method::Get {
        return Err(ApiError::MethodNotAllowed.into());
    }
    ApiResponse::new(SdksResponse {
        sdks: ctx.stash.list_sdks()?.into_iter().map(|x| x.sdk_id()).collect(),
    }, StatusCode::Ok)
}
