mod fault;

use async_trait::async_trait;
use thiserror::Error;
use yaserde::{YaDeserialize, YaSerialize};

pub use fault::{local_name, FaultDetailElement, SoapFault};

#[derive(Debug, Error)]
pub enum Error {
    #[error("Serialization failed: {0}")]
    Serialization(String),
    #[error("Deserialization failed: {0}")]
    Deserialization(String),
    #[error("Authorization failed: {0}")]
    Authorization(String),
    #[error("Redirection error: {0}")]
    Redirection(String),
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Timeout occurred: {0}")]
    Timeout(String),
    /// The device answered with a SOAP Fault (in a 2xx or a non-2xx response).
    #[error("SOAP fault: {0}")]
    Fault(SoapFault),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Other: {0}")]
    Other(String),
}

impl From<Error> for String {
    fn from(error: Error) -> String {
        error.to_string()
    }
}

#[async_trait]
pub trait Transport {
    async fn request(&self, message: &str) -> Result<String, Error>;
}

pub async fn request<T: Transport, R: YaSerialize, S: YaDeserialize>(
    transport: &T,
    request: &R,
) -> Result<S, Error> {
    let ser = |obj: &R| yaserde::ser::to_string(obj).map_err(Error::Serialization);

    let de = |s: &str| yaserde::de::from_str(s).map_err(Error::Deserialization);

    de(&transport
        .request(&crop_xml_declaration(&ser(request)?))
        .await?)
}

fn crop_xml_declaration(xml: &str) -> String {
    xml.split("?>").skip(1).collect()
}

#[test]
fn test_crop_xml_declaration() {
    assert_eq!(
        crop_xml_declaration(r#"<?xml version="1.0" encoding="utf-8"?><element />"#),
        "<element />"
    );
}
