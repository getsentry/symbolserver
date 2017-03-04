use std::fmt;
use std::error;

use hyper::server::Response;
use hyper::status::StatusCode;
use hyper::header::ContentType;
use serde_json;
use serde::Serialize;

use super::super::{Result, Error, ResultExt, ErrorKind};

pub struct ApiResponse {
    body: Vec<u8>,
    status: StatusCode,
}

#[derive(Debug)]
pub enum ApiError {
    NotFound,
    BadRequest,
    MethodNotAllowed,
    PayloadTooLarge,
    BadJson(Box<serde_json::Error>),
    SdkNotFound,
    InternalServerError(Box<Error>),
}

#[derive(Serialize)]
struct ApiErrorDescription {
    #[serde(rename="type")]
    pub ty: String,
    pub message: String,
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

        error!("Internal Server Error: {}", &err);
        if let Some(backtrace) = err.backtrace() {
            debug!("  Traceback: {:?}", backtrace);
        }

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

impl ApiError {
    pub fn get_status(&self) -> StatusCode {
        match *self {
            ApiError::NotFound => StatusCode::NotFound,
            ApiError::BadRequest => StatusCode::BadRequest,
            ApiError::MethodNotAllowed => StatusCode::MethodNotAllowed,
            ApiError::PayloadTooLarge => StatusCode::PayloadTooLarge,
            ApiError::BadJson(_) => StatusCode::BadRequest,
            ApiError::SdkNotFound => StatusCode::NotFound,
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
            }
            ApiError::BadRequest => {
                ApiErrorDescription {
                    ty: "bad_request".into(),
                    message: "The client sent a bad request".into(),
                }
            }
            ApiError::MethodNotAllowed => {
                ApiErrorDescription {
                    ty: "method_not_allowed".into(),
                    message: "This HTTP method is not supported here".into(),
                }
            }
            ApiError::PayloadTooLarge => {
                ApiErrorDescription {
                    ty: "payload_too_large".into(),
                    message: "The request payload is too large".into(),
                }
            }
            ApiError::BadJson(ref json_err) => {
                ApiErrorDescription {
                    ty: "bad_json".into(),
                    message: format!("The client sent bad json: {}", json_err),
                }
            }
            ApiError::SdkNotFound => {
                ApiErrorDescription {
                    ty: "sdk_not_found".into(),
                    message: "The requested SDK was not found".into(),
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
