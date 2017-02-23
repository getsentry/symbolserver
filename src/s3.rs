/// This implements S3 sync for the symbolserver.
///
/// Currently this uses aws-sdk-rust.  The plan is to eventually switch this to
/// rusoto but that does not support streaming from S3 yet.

use rusoto::{ProvideAwsCredentials, AwsCredentials, CredentialsError, Region};
use rusoto::s3::{S3Client, ListObjectsRequest};
use rusoto::default_tls_client;
use chrono::{Duration, UTC};

use std::result::Result as StdResult;

use super::config::Config;
use super::Result;

struct SimpleAwsCredentialsProvider {
    access_key: String,
    secret_key: String,
}

impl SimpleAwsCredentialsProvider {
    pub fn from_config(config: &Config) -> Result<SimpleAwsCredentialsProvider> {
        Ok(SimpleAwsCredentialsProvider {
            access_key: config.get_aws_access_key()?.into(),
            secret_key: config.get_aws_secret_key()?.into(),
        })
    }
}

impl ProvideAwsCredentials for SimpleAwsCredentialsProvider {

    fn credentials(&self) -> StdResult<AwsCredentials, CredentialsError> {
        Ok(AwsCredentials::new(self.access_key.clone(), self.secret_key.clone(),
                               None, UTC::now() + Duration::seconds(600)))
    }
}


pub fn test(config: &Config) -> Result<()> {
    let provider = SimpleAwsCredentialsProvider::from_config(config)?;
    let tls_client = default_tls_client()?;
    let client = S3Client::new(tls_client, provider, Region::UsEast1);

    let mut list_input = ListObjectsRequest::default();
    list_input.bucket = "getsentry-dsyms".into();
    list_input.prefix = Some("mmaps/".into());

    println!("{:?}", client.list_objects(&list_input));

    Ok(())
}
