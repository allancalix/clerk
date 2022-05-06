use hyper::Client;
use plaid_link::State;
use rplaid::client::{Builder, Credentials, Environment};
use rplaid::model::*;

const INSTITUTION_ID: &str = "ins_129571";

fn test_state() -> State {
    State {
        user_id: "test-user".to_string(),
        context: None,
    }
}

#[ignore]
#[tokio::test]
async fn can_execute_exchange_flow() -> Result<(), Box<dyn std::error::Error>> {
    let plaid = Builder::new()
        .with_credentials(Credentials {
            client_id: env!("PLAID_CLIENT_ID").into(),
            secret: env!("PLAID_SECRET").into(),
        })
        .with_env(Environment::Sandbox)
        .build();

    let token = plaid
        .create_public_token(CreatePublicTokenRequest {
            institution_id: INSTITUTION_ID,
            initial_products: &["transactions"],
            options: None,
        })
        .await
        .unwrap();

    let server = plaid_link::LinkServer {
        client: plaid,
        on_exchange: move |link| {
            println!("hello, world!: {:?}", link);
        },
    };

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 0));

    let server = axum::Server::bind(&addr).serve(server.start().into_make_service());
    let addr = server.local_addr();

    tokio::spawn(async move {
        server.await.unwrap();
    });

    let client = Client::new();
    let link_url = format!(
        "http://{}/link?state={}",
        addr.to_string(),
        test_state().to_opaque().unwrap()
    )
    .parse()
    .unwrap();
    let resp = client.get(link_url).await.unwrap();

    assert_eq!(resp.status(), 200);

    let exchange_url = format!(
        "http://{}/exchange/{}?state={}",
        addr.to_string(),
        token,
        test_state().to_opaque().unwrap()
    )
    .parse()
    .unwrap();
    let resp = client.get(exchange_url).await.unwrap();
    assert_eq!(resp.status(), 200);

    Ok(())
}
