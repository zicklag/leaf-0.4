use jacquard::{client::AgentSessionExt, types::string::RecordKey};
use passwords::PasswordGenerator;
use pds_admin_client::{PdsAdminClient, PlcService};

async fn get_client() -> PdsAdminClient {
    for d in &[
        std::env::current_dir().unwrap(),
        std::env::current_dir().unwrap().join(".."),
        std::env::current_dir().unwrap().join("..").join(".."),
    ] {
        let p = d.join(".env");
        if p.exists() {
            dotenvy::from_path(&p).ok();
            continue;
        }
    }
    dotenvy::dotenv().ok();

    let user = std::env::var("ATPROTO_USER").unwrap();
    let password = std::env::var("ATPROTO_PASSWORD").unwrap();
    let client = PdsAdminClient::login(&user, &password).await.unwrap();
    client
}

#[tokio::test]
#[allow(unreachable_code)] // We don't want to run this every time we run cargo test since it creates new accounts.
async fn test_create_account_and_login() {
    return;
    let client = get_client().await;

    // Create account
    let n = PasswordGenerator {
        length: 8,
        numbers: true,
        lowercase_letters: true,
        uppercase_letters: false,
        symbols: false,
        spaces: false,
        exclude_similar_characters: true,
        strict: true,
    }
    .generate_one()
    .expect("gen-suffix");
    let suffix = client
        .handle_suffixes()
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let handle = format!("tq-{n}.{suffix}");
    let session = client.create_account(&handle).await.unwrap();

    println!("✅ logged in as {}", session.info().await.unwrap().0);

    session
        .put_record(
            RecordKey::any("self".into()).unwrap(),
            jacquard::api::app_bsky::actor::profile::Profile::new()
                .display_name(format!("Test account {suffix}"))
                .build(),
        )
        .await
        .unwrap();

    println!("✅ created profile");
}

#[tokio::test]
async fn set_service_endpoints() {
    let client = get_client().await;

    let did = "did:plc:upmu4vl4iedaukqnggjbty34";

    client
        .set_service_endpoints(
            did,
            &[
                PlcService {
                    id: "atproto_pds".into(),
                    r#type: "AtprotoPersonalDataServer".into(),
                    endpoint: client.pds_endpoint().await,
                },
                PlcService {
                    id: "arbiter".into(),
                    r#type: "AtprotoAppView".into(),
                    endpoint: "https://test.localhost".into(),
                },
            ],
        )
        .await
        .unwrap();
}
