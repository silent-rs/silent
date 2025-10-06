use http_body::Body;

pub trait Protocol {
    type Incoming;
    type Outgoing;
    type Body: Body;

    fn into_request(message: Self::Incoming) -> crate::Request;
    fn from_response(response: crate::Response<Self::Body>) -> Self::Outgoing;
}

#[cfg(feature = "server")]
pub mod hyper_http;
