use aws_credential_types::{Credentials, provider::SharedCredentialsProvider};
use aws_sdk_s3::{
    Client,
    config::{Builder, Region},
    primitives::ByteStream,
};
use tracing::error;

use crate::config::Config;

#[derive(Clone)]
pub struct R2Client {
    client: Client,
    bucket: String,
}

impl R2Client {
    pub fn new(config: &Config) -> Result<Self, R2ConfigError> {
        let credentials = Credentials::new(
            config.r2_access_key_id.clone(),
            config.r2_secret_access_key.clone(),
            None,
            None,
            "env",
        );

        let s3_config = Builder::new()
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
            .region(Region::new(config.r2_region.clone()))
            .endpoint_url(config.r2_endpoint.clone())
            .credentials_provider(SharedCredentialsProvider::new(credentials))
            .build();

        Ok(Self {
            client: Client::from_conf(s3_config),
            bucket: config.r2_bucket.clone(),
        })
    }

    pub async fn put_object(
        &self,
        object_key: &str,
        bytes: Vec<u8>,
        content_type: &'static str,
    ) -> Result<(), R2Error> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(object_key)
            .content_type(content_type)
            .body(ByteStream::from(bytes))
            .send()
            .await
            .map_err(|err| R2Error::PutObject(err.to_string()))?;

        Ok(())
    }

    pub async fn get_object(&self, object_key: &str) -> Result<Vec<u8>, R2Error> {
        let output = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(object_key)
            .send()
            .await
            .map_err(|err| {
                let err_text = err.to_string();
                let service_error = err.into_service_error();
                if service_error.is_no_such_key() {
                    error!(
                        "r2 get_object returned NoSuchKey: bucket={}, object_key={}, error={}",
                        self.bucket, object_key, err_text
                    );
                    R2Error::ObjectNotFound
                } else {
                    error!(
                        "r2 get_object failed: bucket={}, object_key={}, error={}",
                        self.bucket, object_key, err_text
                    );
                    R2Error::GetObject(err_text)
                }
            })?;

        output
            .body
            .collect()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|err| R2Error::ReadObjectBody(err.to_string()))
    }

    pub async fn delete_object(&self, object_key: &str) -> Result<(), R2Error> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(object_key)
            .send()
            .await
            .map_err(|err| R2Error::DeleteObject(err.to_string()))?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct R2ConfigError;

impl std::fmt::Display for R2ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "failed to configure Cloudflare R2 client")
    }
}

impl std::error::Error for R2ConfigError {}

#[derive(Debug)]
pub enum R2Error {
    PutObject(String),
    GetObject(String),
    ReadObjectBody(String),
    DeleteObject(String),
    ObjectNotFound,
}

impl std::fmt::Display for R2Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PutObject(err) => write!(f, "failed to upload object to R2: {err}"),
            Self::GetObject(err) => write!(f, "failed to fetch object from R2: {err}"),
            Self::ReadObjectBody(err) => write!(f, "failed to read object body from R2: {err}"),
            Self::DeleteObject(err) => write!(f, "failed to delete object from R2: {err}"),
            Self::ObjectNotFound => write!(f, "object was not found in R2"),
        }
    }
}

impl std::error::Error for R2Error {}
