/// This implements S3 sync for the symbolserver.
///
/// Currently this uses aws-sdk-rust.  The plan is to eventually switch this to
/// rusoto but that does not support streaming from S3 yet.

use std::result::Result as StdResult;
use std::fmt;

use rusoto::{ProvideAwsCredentials, AwsCredentials, CredentialsError,
             ChainProvider};
use rusoto::s3::{S3Client, ListObjectsRequest, Object};
use rusoto::default_tls_client;
use chrono::{Duration, UTC};
use hyper::client::Client as TlsClient;
use url::Url;

use super::sdk::SdkInfo;
use super::config::Config;
use super::memdbstash::RemoteSdk;
use super::{Result, ResultExt};

struct FlexibleCredentialsProvider<'a> {
    chain_provider: ChainProvider,
    config: &'a Config,
}

/// Abstracts over S3 operations
pub struct S3<'a> {
    config: &'a Config,
    url: Url,
    client: S3Client<FlexibleCredentialsProvider<'a>, TlsClient>,
}

impl<'a> ProvideAwsCredentials for FlexibleCredentialsProvider<'a> {

    fn credentials(&self) -> StdResult<AwsCredentials, CredentialsError> {
        let key = self.config.get_aws_access_key();
        let secret = self.config.get_aws_secret_key();
        if key.is_some() && secret.is_some() {
            Ok(AwsCredentials::new(key.unwrap().to_string(),
                                   secret.unwrap().to_string(),
                                   None, UTC::now() + Duration::seconds(3600)))
        } else {
            self.chain_provider.credentials()
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

impl<'a> S3<'a> {

    /// Creates an S3 abstraction from a given config.
    pub fn from_config(config: &'a Config) -> Result<S3<'a>> {
        Ok(S3 {
            config: config,
            url: config.get_aws_bucket_url()?,
            client: S3Client::new(default_tls_client().chain_err(
                    || "Could not configure TLS layer")?, FlexibleCredentialsProvider {
                config: config,
                chain_provider: ChainProvider::new(),
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
        let mut list_input = ListObjectsRequest::default();
        list_input.bucket = self.bucket_name().into();
        list_input.prefix = Some(self.bucket_prefix());
        let out = self.client.list_objects(&list_input)
            .chain_err(|| "Failed to fetch s3 files")?;

        let mut rv = vec![];
        for obj in out.contents.unwrap_or_else(|| vec![]) {
            if let Some(remote_sdk) = self.object_to_remote_sdk(obj) {
                rv.push(remote_sdk);
            }
        }

        Ok(rv)
    }
}
