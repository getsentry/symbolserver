//! This implements S3 sync for the symbolserver.

use std::result::Result as StdResult;
use std::env;
use std::io::{Read, Cursor};

use rusoto::{ProvideAwsCredentials, AwsCredentials, CredentialsError,
             ChainProvider};
use rusoto::s3::{S3Client, ListObjectsRequest, GetObjectRequest, Object,
                 ListObjectsError};
use chrono::{Duration, UTC};
use hyper::client::{Client as HyperClient, ProxyConfig};
use hyper::client::RedirectPolicy;
use hyper::net::{HttpConnector, HttpsConnector};
use hyper_native_tls::NativeTlsClient;
use url::Url;

use super::sdk::SdkInfo;
use super::config::Config;
use super::memdb::stash::RemoteSdk;
use super::{ErrorKind, Result, ResultExt};

struct FlexibleCredentialsProvider {
    chain_provider: ChainProvider,
    access_key: Option<String>,
    secret_key: Option<String>,
}

/// Abstracts over S3 operations
pub struct S3 {
    url: Url,
    client: S3Client<FlexibleCredentialsProvider, HyperClient>,
}

impl ProvideAwsCredentials for FlexibleCredentialsProvider {

    fn credentials(&self) -> StdResult<AwsCredentials, CredentialsError> {
        if_chain! {
            if let Some(ref access_key) = self.access_key;
            if let Some(ref secret_key) = self.secret_key;
            then {
                Ok(AwsCredentials::new(access_key.to_string(),
                                       secret_key.to_string(),
                                       None, UTC::now() + Duration::seconds(3600)))
            } else {
                self.chain_provider.credentials()
            }
        }
    }
}

fn filename_from_key(key: &str) -> Option<&str> {
    key.rsplitn(2, '/').next()
}

fn unquote_etag(quoted_etag: Option<String>) -> Option<String> {
    quoted_etag.and_then(|etag| {
        if etag.len() > 2 && &etag[..1] == "\"" && &etag[etag.len() - 1..] == "\"" {
            Some(etag[1..etag.len() - 1].into())
        } else {
            None
        }
    })
}

pub fn new_hyper_client() -> Result<HyperClient> {
    let ssl = NativeTlsClient::new().chain_err(||
        format!("Couldn't create NativeTlsClient."))?;
    let mut client = if let Ok(proxy_url) = env::var("http_proxy") {
        info!("Using HTTP proxy at {}", proxy_url);
        let proxy : Url = proxy_url.parse()?;
        let proxy_config = ProxyConfig::new(
            "http", proxy.host_str().unwrap().to_string(),
            proxy.port().unwrap(), HttpConnector, ssl);
        HyperClient::with_proxy_config(proxy_config)
    } else {
        let connector = HttpsConnector::new(ssl);
        HyperClient::with_connector(connector)
    };
    client.set_redirect_policy(RedirectPolicy::FollowAll);
    Ok(client)
}

impl S3 {

    /// Creates an S3 abstraction from a given config.
    pub fn from_config(config: &Config) -> Result<S3> {
        Ok(S3 {
            url: config.get_aws_bucket_url()?,
            client: S3Client::new(new_hyper_client().chain_err(
                    || "Could not configure TLS layer")?,
                    FlexibleCredentialsProvider {
                chain_provider: ChainProvider::new(),
                access_key: config.get_aws_access_key().map(|x| x.to_string()),
                secret_key: config.get_aws_secret_key().map(|x| x.to_string()),
            }, config.get_aws_region()?)
        })
    }

    fn bucket_name(&self) -> &str {
        self.url.host_str().unwrap()
    }

    fn bucket_prefix(&self) -> String {
        let path = self.url.path().trim_matches('/');
        if path.len() == 0 {
            "".into()
        } else {
            format!("{}/", path)
        }
    }

    fn object_to_remote_sdk(&self, obj: Object) -> Option<RemoteSdk> {
        if_chain! {
            if let Some(key) = obj.key;
            if key.ends_with(".memdbz");
            if let Some(filename) = filename_from_key(&key);
            if let Some(info) = SdkInfo::from_filename(filename);
            if let Some(etag) = unquote_etag(obj.e_tag);
            if let Some(size) = obj.size;
            then {
                Some(RemoteSdk::new(filename.into(), info, etag, size as u64))
            } else {
                None
            }
        }
    }

    /// Requests the list of all compressed SDKs in the bucket
    pub fn list_upstream_sdks(&self) -> Result<Vec<RemoteSdk>> {
        let mut request = ListObjectsRequest::default();
        request.bucket = self.bucket_name().into();
        request.prefix = Some(self.bucket_prefix());

        // this is the only place where we currently explicitly check for S3
        // HTTP errors because realistically that call is always going ot be
        // the first one that happens.  This gives us better detection in
        // the health check for raw network errors to better report
        // downtime.
        let out = match self.client.list_objects(&request) {
            Ok(out) => out,
            Err(ListObjectsError::HttpDispatch(err)) => {
                return Err(ErrorKind::S3Unavailable(err.to_string()).into());
            }
            Err(err) => {
                return Err(err).chain_err(|| "Failed to fetch SDKs from S3")?;
            }
        };

        let mut rv = vec![];
        for obj in out.contents.unwrap_or_else(|| vec![]) {
            if let Some(remote_sdk) = self.object_to_remote_sdk(obj) {
                rv.push(remote_sdk);
            }
        }

        Ok(rv)
    }

    /// Downloads a given remote SDK and returns a reader to the
    /// bytes in the SDK.
    ///
    /// The files downloaded are XZ compressed.
    pub fn download_sdk(&self, sdk: &RemoteSdk) -> Result<Box<Read>> {
        let request = GetObjectRequest {
            bucket: self.bucket_name().into(),
            key: format!("{}/{}", self.bucket_prefix()
                .trim_right_matches('/'), sdk.filename()),
            response_content_type: Some("application/octet-stream".to_owned()),
            ..Default::default()
        };

        let out = self.client.get_object(&request)
            .chain_err(|| "Failed to fetch SDK from S3")?;

        // XXX: this really should not read into memory but we are currently
        // restricted by rusoto here. https://github.com/rusoto/rusoto/issues/481
        Ok(Box::new(Cursor::new(out.body.unwrap_or_else(|| vec![]))))
    }
}
