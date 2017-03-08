use std::fmt;
use std::error;

use hyper::server::Response;
use hyper::status::StatusCode;
use hyper::header::{Server, ContentLength, ContentType};
use serde_json;
use serde::Serialize;

use super::super::{Result, Error, ResultExt, ErrorKind};
use super::super::constants::VERSION;

/// Represents API responses.
pub struct ApiResponse {
    body: Vec<u8>,
    status: StatusCode,
}

/// Represents API Errors.
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
    /// Creates a new API response.
    pub fn new<S: Serialize>(data: S, status: StatusCode) -> Result<ApiResponse> {
        let mut body = serde_json::to_vec(&data)
            .chain_err(|| "Failed to serialize response for client")?;
        body.push(b'\n');
        Ok(ApiResponse {
            body: body,
            status: status,
        })
    }

    /// Creates an API response from a given error.
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

    /// Writes the API response into a hyper response.
    pub fn write_to_response(&self, is_head: bool, mut resp: Response) -> Result<()> {
        *resp.status_mut() = self.status;
        resp.headers_mut().set(Server(format!("sentry-symbolserver/{}", VERSION)));
        resp.headers_mut().set(ContentLength(self.body.len() as u64));
        resp.headers_mut().set(ContentType::json());
        if !is_head {
            resp.send(&self.body[..])?;
        }
        Ok(())
    }
}

impl ApiError {
    /// Returns the HTTP status code for this error.
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

    /// Converts the error into a response.
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
