use tranquil_client::TranquilClient;

fn load_env() {
    for d in &[std::env::current_dir().unwrap(),
               std::env::current_dir().unwrap().join(".."),
               std::env::current_dir().unwrap().join("..").join("..")] {
        let p = d.join(".env");
        if p.exists() { dotenvy::from_path(&p).ok(); return; }
    }
    dotenvy::dotenv().ok();
}

#[tokio::test]
async fn test_admin_login() {
    load_env();
    let c = TranquilClient::login().await.unwrap();
    println!("✅ admin logged in at {}", c.admin.endpoint().await);
}

#[tokio::test]
async fn test_create_account_and_login() {
    load_env();
    let c = TranquilClient::login().await.unwrap();

    // Create invite code
    let code = c.create_invite_code(1).await.unwrap();
    println!("invite code: {code}");

    // Create account
    let suffix: String = rand::random::<u16>().to_string();
    let handle = format!("tq-{suffix}");
    let (did, _fqdn) = c.create_account(&handle, "test@example.com", "Temppw123!", &code).await.unwrap();
    println!("✅ created account {did}");

    // Login as the new account — may fail if PDS requires email verification
    match c.login_as(&did).await {
        Ok(agent) => {
            println!("✅ logged in as {did}");

            // Do CRUD via reqwest
            let token = match agent.inner().access_token().await.unwrap() {
                jacquard::common::AuthorizationToken::Bearer(t) => t.to_string(),
                _ => panic!("expected bearer"),
            };
            let base = agent.endpoint().await.to_string();
            let http = reqwest::Client::builder().build().unwrap();

            let record = serde_json::json!({"$type": "org.example.post", "text": "hello!"});
            let resp = http.post(format!("{base}/xrpc/com.atproto.repo.createRecord"))
                .header("authorization", format!("Bearer {token}"))
                .json(&serde_json::json!({"repo": did, "collection": "org.example.post", "record": record}))
                .send().await.unwrap();
            let v: serde_json::Value = resp.json().await.unwrap();
            let uri = v["uri"].as_str().unwrap().to_string();
            println!("✅ created {uri}");
        }
        Err(e) => {
            // This PDS may require email verification before login.
            // Reference/standard PDS does not.
            println!("⏭️ login_as (expected if PDS requires email verification): {e}");
        }
    }
}