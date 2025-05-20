use std::marker::PhantomData;

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use serde::Deserialize;

use crate::models::casper::UpdateNotification;
use crate::models::casper::v1::UpdateNotificationResponse;
use crate::models::casper::v1::external_communication_service_server::{
    ExternalCommunicationService, ExternalCommunicationServiceServer,
};

pub struct CommunicationService<T, F> {
    _phantom: PhantomData<T>,
    inner: F,
}

impl<T, F> CommunicationService<T, F> {
    pub const fn new(inner: F) -> Self {
        Self {
            _phantom: PhantomData,
            inner,
        }
    }
}

impl<T, F> CommunicationService<T, F>
where
    Self: ExternalCommunicationService,
{
    pub fn into_service(self) -> ExternalCommunicationServiceServer<Self> {
        ExternalCommunicationServiceServer::new(self)
    }
}

#[tonic::async_trait]
impl<T, F, R> ExternalCommunicationService for CommunicationService<T, F>
where
    T: for<'a> Deserialize<'a>,
    F: Fn(T) -> R,
    R: Future<Output = anyhow::Result<()>> + Send,
    Self: Send + Sync + 'static,
{
    async fn send_notification(
        &self,
        request: tonic::Request<UpdateNotification>,
    ) -> std::result::Result<tonic::Response<UpdateNotificationResponse>, tonic::Status> {
        let request = request.into_inner();

        let decoded = BASE64_STANDARD
            .decode(request.payload)
            .map_err(|err| tonic::Status::from_error(Box::new(err)))?;

        let payload: T = serde_json::from_slice(&decoded)
            .map_err(|err| tonic::Status::from_error(Box::new(err)))?;

        (self.inner)(payload)
            .await
            .map_err(|err| tonic::Status::from_error(err.into()))?;

        Ok(tonic::Response::new(UpdateNotificationResponse {
            message: None,
        }))
    }
}
