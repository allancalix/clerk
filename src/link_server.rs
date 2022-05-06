use std::sync::Arc;

use axum::{
    async_trait,
    extract::{Extension, FromRequest, Path, RequestParts},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use rplaid::client::{Environment, Plaid};
use rplaid::model::*;
use rplaid::HttpClient;
use url::Url;

use crate::plaid::{Link, LinkStatus};
use crate::{CLIENT_NAME, COUNTRY_CODES};

#[derive(Debug, PartialEq)]
pub enum LinkMode {
    Create,
    Update(String),
}

#[async_trait]
impl<B> FromRequest<B> for LinkMode
where
    B: Send,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        let url = Url::options()
            .base_url(Some(&Url::parse("http://localhost").unwrap()))
            .parse(&req.uri().to_string())
            .map_err(|_| (StatusCode::BAD_REQUEST, "invalid_uri"))?;

        let mode = url
            .query_pairs()
            .find(|(key, value)| match (key.as_ref(), value) {
                ("mode", _) => true,
                _ => false,
            });

        let id = url
            .query_pairs()
            .find(|(key, value)| match (key.as_ref(), value) {
                ("token", _) => true,
                _ => false,
            });

        match mode {
            Some((k, v)) => match (k.as_ref(), v.as_ref()) {
                ("mode", "create") => Ok(LinkMode::Create),
                ("mode", "update") => match id {
                    Some(i) => Ok(LinkMode::Update(i.1.to_string())),
                    None => Err((StatusCode::BAD_REQUEST, "update mode must include token")),
                },
                ("mode", _) => Err((StatusCode::BAD_REQUEST, "unsupported mode argument")),
                _ => Ok(LinkMode::Create),
            },
            None => Ok(LinkMode::Create),
        }
    }
}

pub struct LinkServer<T: Fn(Link) + Send + Sync + 'static, S: HttpClient> {
    pub client: Plaid<S>,
    pub on_exchange: T,
}

impl<T: Fn(Link) + Send + Sync + 'static, S: HttpClient> LinkServer<T, S> {
    pub fn start(self) -> Router {
        Router::new()
            .route("/link", get(initialize_link))
            .route("/exchange/:token", get(exchange_token::<T>))
            .layer(Extension(Arc::new(self.client)))
            .layer(Extension(Arc::new(self.on_exchange)))
    }
}

async fn initialize_link(
    mode: LinkMode,
    client: Extension<Arc<Plaid<Box<dyn HttpClient>>>>,
) -> impl IntoResponse {
    let req = match &mode {
        LinkMode::Create => CreateLinkTokenRequest {
            client_name: CLIENT_NAME,
            user: LinkUser::new("test-user"),
            language: "en",
            country_codes: &COUNTRY_CODES,
            products: &crate::PRODUCTS,
            ..CreateLinkTokenRequest::default()
        },
        LinkMode::Update(token) => CreateLinkTokenRequest {
            client_name: CLIENT_NAME,
            user: LinkUser::new("test-user"),
            language: "en",
            country_codes: &COUNTRY_CODES,
            access_token: Some(&token),
            ..CreateLinkTokenRequest::default()
        },
    };

    match client.create_link_token(&req).await {
        Ok(r) => Html(format!(
            r#"
                    <!DOCTYPE html>
                    <script src="https://cdn.plaid.com/link/v2/stable/link-initialize.js"></script>
                    <body></body>
                    <script>var handler = Plaid.create({{
                        token: "{}",
                        onSuccess: (public_token, metadata) => {{
                            window.location.href = `/exchange/${{public_token}}`
                        }},
                        onLoad: () => null,
                        onExit: (event_name, metadata) => null,
                        receivedRedirectUri: null,
                    }}); handler.open();</script>
                    </DOCTYPE>
                    "#,
            r.link_token
        )),
        Err(err) => Html(format!("unexpected error {:?}", err)),
    }
}

async fn exchange_token<T: Fn(Link) + Send + Sync + 'static>(
    Path(token): Path<String>,
    on_exchange: Extension<Arc<T>>,
    client: Extension<Arc<Plaid<Box<dyn HttpClient>>>>,
) -> impl IntoResponse {
    let res = client.exchange_public_token(token).await.unwrap();

    on_exchange(Link {
        alias: "test".to_string(),
        access_token: res.access_token,
        item_id: res.item_id,
        state: LinkStatus::Active,
        env: Environment::Sandbox,
    });

    Html("OK")
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::extract::RequestParts;
    use http::Uri;

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
            assert_eq!(LinkMode::from_request(&mut req).await, Ok(t.1))
        }
    }

    #[tokio::test]
    async fn extract_mode_from_query_rejects_invalid_params() {
        let tests = vec![
            (
                "http://localhost:4000/init?mode=invalid",
                Err((StatusCode::BAD_REQUEST, "unsupported mode argument")),
            ),
            (
                "http://localhost:4000/init?mode=update",
                Err((StatusCode::BAD_REQUEST, "update mode must include token")),
            ),
        ];

        for t in tests {
            let mut req = request_parts_from_uri(t.0);
            assert_eq!(LinkMode::from_request(&mut req).await, t.1)
        }
    }
}
