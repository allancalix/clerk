use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    async_trait,
    extract::{Extension, FromRequest, Path, RequestParts},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use lazy_static::lazy_static;
use rplaid::{client::Plaid, model::*, HttpClient};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

static CLIENT_NAME: &str = "clerk";
static PRODUCTS: [&str; 1] = ["transactions"];
static COUNTRY_CODES: [&str; 1] = ["US"];

lazy_static! {
    // HACK: Url doesn't provide a good way to initialize a Url from a relative
    // path and axum uri returns only the path partial. __Do not depend on the host,
    // scheme, or any non path part of the Url constructed with this as a base.__
    static ref BASE_URL: Url = {
        Url::parse("http://localhost").unwrap()
    };
}

#[derive(Debug, Error)]
pub enum LinkError {
    #[error("{0}")]
    InvalidArgument(String),
    #[error("unable to parse argument")]
    ParseError(#[from] serde_json::Error),
    #[error("failed to decode base64 argument")]
    DecodeError(#[from] base64::DecodeError),
    #[error("upstream link call failed")]
    LinkClientError(#[from] rplaid::client::ClientError),
    #[error("invalid string source")]
    BadRequest(#[from] std::string::FromUtf8Error),
}

impl IntoResponse for LinkError {
    fn into_response(self) -> Response {
        match self {
            LinkError::InvalidArgument(s) => (StatusCode::BAD_REQUEST, Html(s)),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html("something really bad happened".into()),
            ),
        }
        .into_response()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum LinkMode {
    Create,
    Update(String),
}

#[async_trait]
impl<B> FromRequest<B> for LinkMode
where
    B: Send,
{
    type Rejection = LinkError;

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        let url = Url::options()
            .base_url(Some(&BASE_URL))
            .parse(&req.uri().to_string())
            .map_err(|_| LinkError::InvalidArgument("invalid uri".into()))?;

        let mode = url
            .query_pairs()
            .find(|(key, value)| matches!((key.as_ref(), value), ("mode", _)));

        let id = url
            .query_pairs()
            .find(|(key, value)| matches!((key.as_ref(), value), ("token", _)));

        match mode {
            Some((k, v)) => match (k.as_ref(), v.as_ref()) {
                ("mode", "create") => Ok(LinkMode::Create),
                ("mode", "update") => match id {
                    Some(i) => Ok(LinkMode::Update(i.1.to_string())),
                    None => Err(LinkError::InvalidArgument(
                        "update mode must include token".into(),
                    )),
                },
                ("mode", _) => Err(LinkError::InvalidArgument(
                    "unsupported mode argument".into(),
                )),
                _ => Ok(LinkMode::Create),
            },
            None => Ok(LinkMode::Create),
        }
    }
}

/// State can be used to curry data during the link flow lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct State {
    /// A system-specific user ID for which the credentials are being created.
    pub user_id: String,
    /// Arbitrary key value pairs containing metadata about the exchange request.
    pub context: Option<HashMap<String, String>>,
}

impl State {
    pub fn to_opaque(self) -> Result<String, serde_json::Error> {
        Ok(base64::encode_config(
            serde_json::to_string(&self)?.as_bytes(),
            base64::URL_SAFE,
        ))
    }
}

#[async_trait]
impl<B> FromRequest<B> for State
where
    B: Send,
{
    type Rejection = LinkError;

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        let url = Url::options()
            .base_url(Some(&BASE_URL))
            .parse(&req.uri().to_string())
            .map_err(|_| LinkError::InvalidArgument("invalid_uri".into()))?;

        let state = url
            .query_pairs()
            .find(|(key, value)| matches!((key.as_ref(), value), ("state", _)));

        match state {
            Some((k, v)) => match (k.as_ref(), v.as_ref()) {
                ("state", token) => Ok(serde_json::from_str(&String::from_utf8(
                    base64::decode_config(token.as_bytes(), base64::URL_SAFE)?,
                )?)?),
                _ => unimplemented!(),
            },
            None => Ok(Self {
                user_id: "".to_string(),
                context: None,
            }),
        }
    }
}

/// Token are a set of credentials for the given `item_id`.
#[derive(Debug, Clone)]
pub struct Token {
    /// The Plaid item ID the access token belongs to.
    pub item_id: String,
    /// The access token to access data for the item.
    pub access_token: String,
    /// Plaid link-flow state context.
    pub state: State,
}

use tokio::sync::broadcast;

pub struct LinkServer<S: HttpClient> {
    pub client: Plaid<S>,
    pub link_channel: broadcast::Sender<Token>,
    pub listener: broadcast::Receiver<Token>,
}

impl<S: HttpClient> LinkServer<S> {
    pub fn new(client: Plaid<S>) -> Self {
        let (tx, rx) = broadcast::channel(1);

        Self {
            client,
            link_channel: tx,
            listener: rx,
        }
    }

    pub fn on_exchange(&self) -> broadcast::Receiver<Token> {
        self.link_channel.subscribe()
    }

    pub fn start(self) -> Router {
        Router::new()
            .route("/link", get(initialize_link))
            .route("/exchange/:token", get(exchange_token))
            .layer(Extension(Arc::new(self.client)))
            .layer(Extension(self.link_channel))
    }
}

async fn initialize_link(
    mode: LinkMode,
    state: State,
    client: Extension<Arc<Plaid<Box<dyn HttpClient>>>>,
) -> impl IntoResponse {
    let req = match &mode {
        LinkMode::Create => CreateLinkTokenRequest {
            client_name: CLIENT_NAME,
            user: LinkUser::new(&state.user_id),
            language: "en",
            country_codes: &COUNTRY_CODES,
            products: &crate::PRODUCTS,
            ..CreateLinkTokenRequest::default()
        },
        LinkMode::Update(token) => CreateLinkTokenRequest {
            client_name: CLIENT_NAME,
            user: LinkUser::new(&state.user_id),
            language: "en",
            country_codes: &COUNTRY_CODES,
            access_token: Some(token),
            ..CreateLinkTokenRequest::default()
        },
    };

    match client.create_link_token(&req).await {
        Ok(r) => Ok(Html(format!(
            r#"
                    <!DOCTYPE html>
                    <script src="https://cdn.plaid.com/link/v2/stable/link-initialize.js"></script>
                    <body></body>
                    <script>var handler = Plaid.create({{
                        token: "{}",
                        onSuccess: (public_token, metadata) => {{
                            window.location.href = `/exchange/${{public_token}}?state={}`
                        }},
                        onLoad: () => null,
                        onExit: (event_name, metadata) => null,
                        receivedRedirectUri: null,
                    }}); handler.open();</script>
                    </DOCTYPE>
                    "#,
            r.link_token,
            state.to_opaque().map_err(LinkError::ParseError)?,
        ))),
        Err(err) => Err(LinkError::InvalidArgument(format!(
            "unexpected error {:?}",
            err
        ))),
    }
}

async fn exchange_token<'a>(
    Path(token): Path<String>,
    state: State,
    client: Extension<Arc<Plaid<Box<dyn HttpClient>>>>,
    on_exchange: Extension<broadcast::Sender<Token>>,
) -> Result<Html<&'a str>, LinkError> {
    let res = client
        .exchange_public_token(token)
        .await
        .map_err(LinkError::LinkClientError)?;

    on_exchange
        .send(Token {
            item_id: res.item_id,
            access_token: res.access_token,
            state,
        })
        .unwrap();

    Ok(Html("OK"))
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::extract::RequestParts;

    fn request_parts_from_uri(uri: &str) -> RequestParts<()> {
        RequestParts::new(http::request::Request::builder().uri(uri).body(()).unwrap())
    }

    #[tokio::test]
    async fn extract_mode_from_query() {
        let tests = vec![
            ("http://localhost:4000/init", LinkMode::Create),
            ("http://localhost:4000/init?mode=create", LinkMode::Create),
            (
                "http://localhost:4000/init?mode=create&token=foobar",
                LinkMode::Create,
            ),
            (
                "http://localhost:4000/init?mode=update&token=foobar",
                LinkMode::Update("foobar".to_string()),
            ),
        ];

        for t in tests {
            let mut req = request_parts_from_uri(t.0);
            assert_eq!(LinkMode::from_request(&mut req).await.unwrap(), t.1)
        }
    }

    #[tokio::test]
    async fn extract_mode_from_query_rejects_invalid_params() {
        let tests = vec![
            (
                "http://localhost:4000/init?mode=invalid",
                LinkError::InvalidArgument("unsupported mode argument".into()),
            ),
            (
                "http://localhost:4000/init?mode=update",
                LinkError::InvalidArgument("update mode must include token".into()),
            ),
        ];

        for t in tests {
            let mut req = request_parts_from_uri(t.0);
            assert_eq!(
                LinkMode::from_request(&mut req)
                    .await
                    .unwrap_err()
                    .to_string(),
                t.1.to_string()
            )
        }
    }

    #[tokio::test]
    async fn extract_state_from_query_param() {
        let state = State {
            user_id: "foobar@tester.com".to_string(),
            context: None,
        };

        let mut req = request_parts_from_uri(&format!(
            "http://localhost:4000/init?state={}",
            state.clone().to_opaque().unwrap()
        ));
        assert_eq!(State::from_request(&mut req).await.unwrap(), state)
    }

    #[tokio::test]
    async fn init_without_state_params_provides_default() {
        let state = State {
            user_id: "".to_string(),
            context: None,
        };

        let mut req = request_parts_from_uri("http://localhost:4000/init");
        assert_eq!(State::from_request(&mut req).await.unwrap(), state)
    }
}
