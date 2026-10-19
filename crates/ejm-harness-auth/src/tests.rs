use std::sync::atomic::{AtomicBool, Ordering};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use wiremock::{
    Mock, MockGuard, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use super::*;
use crate::{FileStore, StoredAuth, VaultKeyStore};

const CLIENT: &str = "oaiapp_test_client";
const CALLBACK: &str = "http://127.0.0.1:54823/auth/callback";
const KEY: &[u8] = include_bytes!("../tests/fixtures/signing-key.der");

#[derive(Default)]
struct FixtureKeys(std::sync::Mutex<Option<SecretString>>);

impl VaultKeyStore for FixtureKeys {
    fn get(&self, _: &str) -> Result<Option<SecretString>> {
        Ok(self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("fixture key lock"))?
            .clone())
    }
    fn set(&self, _: &str, key: &SecretString) -> Result<()> {
        *self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("fixture key lock"))? = Some(key.clone());
        Ok(())
    }
}

struct Fixture {
    server: MockServer,
    engine: AuthEngine,
    store: Arc<FileStore>,
    directory: tempfile::TempDir,
    keys: Arc<FixtureKeys>,
}

impl Fixture {
    async fn new() -> Result<Self> {
        let server = MockServer::start().await;
        let origin = server.uri();
        Mock::given(method("GET"))
            .and(path("/discovery"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issuer": ISSUER,
                "authorization_endpoint": format!("{origin}/authorize"),
                "token_endpoint": format!("{origin}/token"),
                "jwks_uri": format!("{origin}/jwks"),
                "revocation_endpoint": format!("{origin}/revoke"),
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                include_str!("../tests/fixtures/jwks.json"),
                "application/json",
            ))
            .mount(&server)
            .await;
        let directory = tempfile::tempdir()?;
        let keys = Arc::new(FixtureKeys::default());
        let store = Arc::new(FileStore::open_with_key_store(
            directory.path().join("auth"),
            keys.clone(),
        )?);
        let engine = test_engine(&server, store.clone())?;
        Ok(Self {
            server,
            engine,
            store,
            directory,
            keys,
        })
    }

    async fn token(&self, status: u16, body: Value) -> MockGuard {
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .expect(1)
            .mount_as_scoped(&self.server)
            .await
    }

    async fn login(&self, subject: &str, scopes: &str) -> Result<RegistrationStatus> {
        let attempt = self
            .engine
            .start(CALLBACK, None, CancellationToken::new())
            .await?;
        let body = token_body(&attempt, subject, scopes)?;
        let _token = self.token(200, body).await;
        self.engine
            .complete(
                &attempt.id,
                &callback(&attempt, Some(CLIENT))?,
                CancellationToken::new(),
            )
            .await
    }

    async fn state(&self) -> Result<StoredAuth> {
        self.store.lock(CancellationToken::new()).await?.load()
    }

    async fn expire(&self) -> Result<()> {
        let mut transaction = self.store.lock(CancellationToken::new()).await?;
        let mut stored = transaction.load()?;
        let registration = stored
            .registrations
            .first_mut()
            .context("registration missing")?;
        registration
            .credentials
            .as_mut()
            .context("credentials missing")?
            .expires_at = Some(timestamp().saturating_sub(1));
        transaction.save(&stored)
    }

    async fn token_forms(&self) -> Result<Vec<HashMap<String, String>>> {
        self.server
            .received_requests()
            .await
            .context("request capture disabled")?
            .into_iter()
            .filter(|request| request.url.path() == "/token")
            .map(|request| {
                let body = String::from_utf8(request.body)?;
                Ok(Url::parse(&format!("http://localhost/?{body}"))?
                    .query_pairs()
                    .into_owned()
                    .collect())
            })
            .collect()
    }
}

fn test_engine(server: &MockServer, store: Arc<dyn CredentialStore>) -> Result<AuthEngine> {
    let http = Arc::new(Http {
        client: reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()?,
        slots: Semaphore::new(4),
        discovery_url: format!("{}/discovery", server.uri()),
        allowed_origin: server.uri(),
    });
    Ok(AuthEngine {
        inner: Arc::new(Inner {
            config: AuthConfig {
                agent_name: "ejm-cli".into(),
            },
            store,
            identity: IdentityValidator::new(http.clone()),
            http,
            discovery: AsyncMutex::new(None),
            attempts: Mutex::new(HashMap::new()),
        }),
    })
}

fn query(attempt: &LoginAttempt) -> Result<HashMap<String, String>> {
    Ok(Url::parse(attempt.authorization_url.expose_secret())?
        .query_pairs()
        .into_owned()
        .collect())
}

fn callback(attempt: &LoginAttempt, client: Option<&str>) -> Result<String> {
    let values = query(attempt)?;
    let mut url = Url::parse(&attempt.redirect_uri)?;
    url.query_pairs_mut()
        .append_pair("state", values.get("state").context("state missing")?)
        .append_pair("code", "test-code");
    if let Some(client) = client {
        url.query_pairs_mut().append_pair("client_id", client);
    }
    Ok(url.into())
}

fn claims(subject: &str, nonce: Option<&str>) -> Value {
    let now = timestamp();
    json!({ "iss": ISSUER, "sub": subject, "aud": CLIENT, "exp": now + 3600,
        "iat": now, "nonce": nonce, "email": "same@example.test" })
}

fn signed(claims: &Value) -> Result<String> {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test-key".into());
    Ok(encode(&header, claims, &EncodingKey::from_rsa_der(KEY))?)
}

fn token_body(attempt: &LoginAttempt, subject: &str, scopes: &str) -> Result<Value> {
    let query = query(attempt)?;
    let nonce = query.get("nonce").context("nonce missing")?;
    Ok(
        json!({ "access_token": "access-original", "refresh_token": "refresh-original",
        "id_token": signed(&claims(subject, Some(nonce)))?, "token_type": "Bearer",
        "expires_in": 3600, "scope": scopes }),
    )
}

#[tokio::test]
async fn full_sign_in_validates_identity_pkce_and_returning_binding() -> Result<()> {
    let fixture = Fixture::new().await?;
    let attempt = fixture
        .engine
        .start(CALLBACK, None, CancellationToken::new())
        .await?;
    let parameters = query(&attempt)?;
    assert_eq!(
        parameters.get("client_id").map(String::as_str),
        Some(DYNAMIC_CLIENT)
    );
    assert_eq!(
        parameters.get("agent_name_hint").map(String::as_str),
        Some("ejm-cli")
    );
    assert_eq!(
        parameters.get("code_challenge_method").map(String::as_str),
        Some("S256")
    );
    assert!(!parameters.contains_key("id_token_hint") && !parameters.contains_key("login_hint"));
    let _token = fixture
        .token(200, token_body(&attempt, "account-a", SCOPES)?)
        .await;
    let status = fixture
        .engine
        .complete(
            &attempt.id,
            &callback(&attempt, Some(CLIENT))?,
            CancellationToken::new(),
        )
        .await?;
    assert!(status.signed_in && status.plan_enabled);
    assert_eq!(status.client_id, CLIENT);
    assert_eq!(status.subject, "account-a");
    assert!(
        fixture
            .engine
            .complete(
                &attempt.id,
                &callback(&attempt, Some(CLIENT))?,
                CancellationToken::new()
            )
            .await
            .is_err()
    );
    let forms = fixture.token_forms().await?;
    let form = forms.first().context("exchange not captured")?;
    assert_eq!(form.get("client_id").map(String::as_str), Some(CLIENT));
    assert_eq!(form.get("redirect_uri").map(String::as_str), Some(CALLBACK));
    let verifier = form.get("code_verifier").context("PKCE verifier missing")?;
    assert_eq!(
        parameters.get("code_challenge"),
        Some(&URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())))
    );
    let returning = fixture
        .engine
        .start(CALLBACK, Some(&status.id), CancellationToken::new())
        .await?;
    let returning_parameters = query(&returning)?;
    assert_eq!(
        returning_parameters.get("client_id").map(String::as_str),
        Some(CLIENT)
    );
    assert_eq!(
        returning_parameters.get("ext_agent_host_id"),
        parameters.get("ext_agent_host_id")
    );
    assert_ne!(returning_parameters.get("state"), parameters.get("state"));
    assert_ne!(returning_parameters.get("nonce"), parameters.get("nonce"));
    assert!(!returning_parameters.contains_key("agent_name_hint"));
    assert!(fixture.engine.cancel(&returning.id));
    let metadata = serde_json::to_string(&status)?;
    assert!(!metadata.contains("access-original") && !metadata.contains("refresh-original"));
    assert!(!format!("{:?}", fixture.state().await?).contains("access-original"));
    Ok(())
}

#[tokio::test]
async fn invalid_grant_retries_issued_client_without_persisting_identity() -> Result<()> {
    let fixture = Fixture::new().await?;
    let first = fixture
        .engine
        .start(CALLBACK, None, CancellationToken::new())
        .await?;
    {
        let _token = fixture.token(400, json!({"error": "invalid_grant"})).await;
        let error = fixture
            .engine
            .complete(
                &first.id,
                &callback(&first, Some(CLIENT))?,
                CancellationToken::new(),
            )
            .await
            .err()
            .context("exchange should fail")?;
        assert_eq!(
            error
                .downcast_ref::<RetryRequired>()
                .context("typed retry missing")?
                .attempt_id,
            first.id
        );
    }
    assert!(fixture.state().await?.registrations.is_empty());
    let second = fixture
        .engine
        .retry(&first.id, CALLBACK, CancellationToken::new())
        .await?;
    let first_parameters = query(&first)?;
    let second_parameters = query(&second)?;
    assert_eq!(
        second_parameters.get("client_id").map(String::as_str),
        Some(CLIENT)
    );
    assert!(!second_parameters.contains_key("agent_name_hint"));
    for key in ["state", "nonce", "code_challenge"] {
        assert_ne!(first_parameters.get(key), second_parameters.get(key));
    }
    assert!(
        fixture
            .engine
            .retry(&first.id, CALLBACK, CancellationToken::new())
            .await
            .is_err()
    );
    let _token = fixture
        .token(200, token_body(&second, "account-a", SCOPES)?)
        .await;
    let status = fixture
        .engine
        .complete(
            &second.id,
            &callback(&second, None)?,
            CancellationToken::new(),
        )
        .await?;
    assert!(status.plan_enabled);
    Ok(())
}

#[tokio::test]
async fn callbacks_reject_state_uri_duplicates_client_changes_and_cancellation() -> Result<()> {
    let fixture = Fixture::new().await?;
    for suffix in ["&state=duplicate", "&error=access_denied"] {
        let attempt = fixture
            .engine
            .start(CALLBACK, None, CancellationToken::new())
            .await?;
        let callback = format!("{}{suffix}", callback(&attempt, Some(CLIENT))?);
        assert!(
            fixture
                .engine
                .complete(&attempt.id, &callback, CancellationToken::new())
                .await
                .is_err()
        );
    }
    let attempt = fixture
        .engine
        .start(CALLBACK, None, CancellationToken::new())
        .await?;
    let callback = callback(&attempt, Some(CLIENT))?.replace("state=", "different=");
    assert!(
        fixture
            .engine
            .complete(&attempt.id, &callback, CancellationToken::new())
            .await
            .is_err()
    );
    let attempt = fixture
        .engine
        .start(CALLBACK, None, CancellationToken::new())
        .await?;
    assert!(
        fixture
            .engine
            .complete(
                &attempt.id,
                &callback.replace(":54823", ":54824"),
                CancellationToken::new()
            )
            .await
            .is_err()
    );
    for uri in [
        "http://localhost:42/auth/callback",
        "http://127.0.0.1:0/auth/callback",
        "https://127.0.0.1:42/auth/callback",
        "http://127.0.0.1:42/wrong",
    ] {
        assert!(
            fixture
                .engine
                .start(uri, None, CancellationToken::new())
                .await
                .is_err()
        );
    }
    let attempt = fixture
        .engine
        .start(CALLBACK, None, CancellationToken::new())
        .await?;
    assert!(fixture.engine.cancel(&attempt.id));
    assert!(
        fixture
            .engine
            .complete(&attempt.id, CALLBACK, CancellationToken::new())
            .await
            .is_err()
    );
    assert_eq!(fixture.token_forms().await?.len(), 0);
    Ok(())
}

#[tokio::test]
async fn signed_identity_rejects_invalid_claims_and_signatures() -> Result<()> {
    let fixture = Fixture::new().await?;
    for invalid in [
        "nonce",
        "audience",
        "issuer",
        "expired",
        "issued",
        "signature",
        "algorithm",
        "authorized",
    ] {
        let attempt = fixture
            .engine
            .start(CALLBACK, None, CancellationToken::new())
            .await?;
        let parameters = query(&attempt)?;
        let mut claim = claims("account-a", parameters.get("nonce").map(String::as_str));
        match invalid {
            "nonce" => claim["nonce"] = json!("wrong"),
            "audience" => claim["aud"] = json!("another-client"),
            "issuer" => claim["iss"] = json!("https://example.test"),
            "expired" => {
                claim["exp"] = json!(timestamp().saturating_sub(60));
                claim["iat"] = json!(timestamp().saturating_sub(3600));
            }
            "issued" => claim["iat"] = json!(timestamp() + 600),
            "authorized" => claim["azp"] = json!("another-client"),
            _ => {}
        }
        let mut token = signed(&claim)?;
        if invalid == "signature" {
            let index = token.rfind('.').context("signature missing")? + 1;
            let replacement = if token.as_bytes()[index] == b'A' {
                "B"
            } else {
                "A"
            };
            token.replace_range(index..=index, replacement);
        }
        if invalid == "algorithm" {
            token = encode(
                &Header::new(Algorithm::HS256),
                &claim,
                &EncodingKey::from_secret(b"wrong-key"),
            )?;
        }
        let mut body = token_body(&attempt, "account-a", SCOPES)?;
        body["id_token"] = json!(token);
        let _token = fixture.token(200, body).await;
        let error = fixture
            .engine
            .complete(
                &attempt.id,
                &callback(&attempt, Some(CLIENT))?,
                CancellationToken::new(),
            )
            .await
            .err()
            .context("identity must be rejected")?;
        assert!(!format!("{error:#}").contains("access-original"));
        assert!(fixture.state().await?.registrations.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn returning_account_rejects_subject_and_callback_client_changes() -> Result<()> {
    let fixture = Fixture::new().await?;
    let status = fixture.login("account-a", SCOPES).await?;
    let attempt = fixture
        .engine
        .start(CALLBACK, Some(&status.id), CancellationToken::new())
        .await?;
    assert!(
        fixture
            .engine
            .complete(
                &attempt.id,
                &callback(&attempt, Some("oaiapp_other"))?,
                CancellationToken::new()
            )
            .await
            .is_err()
    );
    let attempt = fixture
        .engine
        .start(CALLBACK, Some(&status.id), CancellationToken::new())
        .await?;
    let _token = fixture
        .token(200, token_body(&attempt, "account-b", SCOPES)?)
        .await;
    assert!(
        fixture
            .engine
            .complete(
                &attempt.id,
                &callback(&attempt, None)?,
                CancellationToken::new()
            )
            .await
            .is_err()
    );
    assert_eq!(
        fixture
            .engine
            .status(&status.id, CancellationToken::new())
            .await?
            .subject,
        "account-a"
    );
    assert_eq!(fixture.state().await?.registrations.len(), 1);
    Ok(())
}

#[tokio::test]
async fn identity_without_plan_grant_is_saved_but_cannot_infer() -> Result<()> {
    let fixture = Fixture::new().await?;
    let status = fixture.login("account-a", "openid profile email").await?;
    assert!(status.signed_in && !status.plan_enabled);
    assert!(
        fixture
            .engine
            .source(&status.id)
            .access_token(CancellationToken::new())
            .await
            .is_err()
    );
    assert_eq!(
        fixture
            .engine
            .registrations(CancellationToken::new())
            .await?
            .len(),
        1
    );
    assert_eq!(fixture.token_forms().await?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn separate_engines_serialize_rotating_refresh_and_persist_replacement() -> Result<()> {
    let fixture = Fixture::new().await?;
    let status = fixture.login("account-a", SCOPES).await?;
    fixture.expire().await?;
    let independent = test_engine(
        &fixture.server,
        Arc::new(FileStore::open_with_key_store(
            fixture.directory.path().join("auth"),
            fixture.keys.clone(),
        )?),
    )?;
    let body = json!({"access_token": "access-next", "refresh_token": "refresh-next", "token_type": "Bearer", "expires_in": 3600, "id_token": signed(&claims("account-a", None))?});
    let _token = fixture.token(200, body).await;
    let left = fixture.engine.source(&status.id);
    let right = independent.source(&status.id);
    let (left, right) = tokio::join!(
        left.access_token(CancellationToken::new()),
        right.access_token(CancellationToken::new())
    );
    assert_eq!(left?.expose_secret(), "access-next");
    assert_eq!(right?.expose_secret(), "access-next");
    let stored = fixture.state().await?;
    let registration = stored
        .registrations
        .first()
        .context("registration missing")?;
    assert!(!registration.refresh_pending);
    assert_eq!(
        registration
            .credentials
            .as_ref()
            .context("credentials missing")?
            .refresh_token
            .as_ref()
            .context("refresh missing")?
            .expose_secret(),
        "refresh-next"
    );
    let forms = fixture.token_forms().await?;
    assert_eq!(forms.len(), 2);
    let renewal = forms.last().context("refresh missing")?;
    assert_eq!(
        renewal.get("refresh_token").map(String::as_str),
        Some("refresh-original")
    );
    assert!(!renewal.contains_key("scope"));
    assert_eq!(renewal.get("client_id").map(String::as_str), Some(CLIENT));
    Ok(())
}

#[tokio::test]
async fn malformed_renewal_keeps_durable_uncertainty_marker() -> Result<()> {
    let fixture = Fixture::new().await?;
    let status = fixture.login("account-a", SCOPES).await?;
    fixture.expire().await?;
    let _token = fixture
        .token(200, json!({"access_token": "access-unconfirmed"}))
        .await;
    assert!(
        fixture
            .engine
            .source(&status.id)
            .access_token(CancellationToken::new())
            .await
            .is_err()
    );
    assert!(
        fixture
            .state()
            .await?
            .registrations
            .first()
            .context("registration missing")?
            .refresh_pending
    );
    let reopened = test_engine(
        &fixture.server,
        Arc::new(FileStore::open_with_key_store(
            fixture.directory.path().join("auth"),
            fixture.keys.clone(),
        )?),
    )?;
    assert!(
        reopened
            .source(&status.id)
            .access_token(CancellationToken::new())
            .await
            .is_err()
    );
    assert!(
        !reopened
            .status(&status.id, CancellationToken::new())
            .await?
            .signed_in
    );
    assert_eq!(fixture.token_forms().await?.len(), 2);
    Ok(())
}

#[tokio::test]
async fn cancelled_completion_never_persists_credentials() -> Result<()> {
    let fixture = Fixture::new().await?;
    let attempt = fixture
        .engine
        .start(CALLBACK, None, CancellationToken::new())
        .await?;
    let response = token_body(&attempt, "account-a", SCOPES)?;
    let _token = Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(response)
                .set_delay(Duration::from_millis(150)),
        )
        .mount_as_scoped(&fixture.server)
        .await;
    let engine = fixture.engine.clone();
    let id = attempt.id.clone();
    let callback = callback(&attempt, Some(CLIENT))?;
    let completion = tokio::spawn(async move {
        engine
            .complete(&id, &callback, CancellationToken::new())
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        while fixture.token_forms().await?.is_empty() {
            tokio::task::yield_now().await;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    assert!(fixture.engine.cancel(&attempt.id));
    assert!(completion.await?.is_err());
    assert!(fixture.state().await?.registrations.is_empty());
    Ok(())
}

#[tokio::test]
async fn logout_clears_local_tokens_when_remote_revoke_fails() -> Result<()> {
    let fixture = Fixture::new().await?;
    let status = fixture.login("account-a", SCOPES).await?;
    let _revoke = Mock::given(method("POST"))
        .and(path("/revoke"))
        .respond_with(ResponseTemplate::new(503))
        .expect(3)
        .mount_as_scoped(&fixture.server)
        .await;
    let result = fixture
        .engine
        .logout(&status.id, CancellationToken::new())
        .await?;
    assert!(!result.remote_revoked);
    let state = fixture.state().await?;
    let registration = state
        .registrations
        .first()
        .context("registration missing")?;
    assert!(registration.credentials.is_none());
    assert_eq!(registration.client_id, CLIENT);
    assert_eq!(registration.subject, "account-a");
    assert!(
        !fixture
            .engine
            .status(&status.id, CancellationToken::new())
            .await?
            .signed_in
    );
    Ok(())
}

#[tokio::test]
async fn logout_invalidates_pending_sign_in_and_retry_across_engines() -> Result<()> {
    let fixture = Fixture::new().await?;
    let status = fixture.login("account-a", SCOPES).await?;
    let independent = test_engine(
        &fixture.server,
        Arc::new(FileStore::open_with_key_store(
            fixture.directory.path().join("auth"),
            fixture.keys.clone(),
        )?),
    )?;
    let pending = independent
        .start(CALLBACK, Some(&status.id), CancellationToken::new())
        .await?;
    let retrying = independent
        .start(CALLBACK, Some(&status.id), CancellationToken::new())
        .await?;
    {
        let _token = fixture.token(400, json!({"error": "invalid_grant"})).await;
        let error = independent
            .complete(
                &retrying.id,
                &callback(&retrying, None)?,
                CancellationToken::new(),
            )
            .await
            .err()
            .context("retry rejection missing")?;
        assert!(error.downcast_ref::<RetryRequired>().is_some());
    }
    let _revoke = Mock::given(method("POST"))
        .and(path("/revoke"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount_as_scoped(&fixture.server)
        .await;
    assert!(
        fixture
            .engine
            .logout(&status.id, CancellationToken::new())
            .await?
            .remote_revoked
    );
    assert!(
        independent
            .retry(&retrying.id, CALLBACK, CancellationToken::new())
            .await
            .is_err()
    );
    let _token = fixture
        .token(200, token_body(&pending, "account-a", SCOPES)?)
        .await;
    assert!(
        independent
            .complete(
                &pending.id,
                &callback(&pending, None)?,
                CancellationToken::new()
            )
            .await
            .is_err()
    );
    let stored = fixture.state().await?;
    let registration = stored
        .registrations
        .first()
        .context("registration missing")?;
    assert_eq!(registration.authorization_revision, 1);
    assert!(registration.credentials.is_none());
    assert!(
        !independent
            .status(&status.id, CancellationToken::new())
            .await?
            .signed_in
    );
    Ok(())
}

struct FailingStore {
    inner: Arc<FileStore>,
    enabled: Arc<AtomicBool>,
}

struct FailingTransaction {
    inner: Box<dyn CredentialTransaction>,
    enabled: Arc<AtomicBool>,
}

#[async_trait]
impl CredentialStore for FailingStore {
    async fn lock(&self, cancel: CancellationToken) -> Result<Box<dyn CredentialTransaction>> {
        Ok(Box::new(FailingTransaction {
            inner: self.inner.lock(cancel).await?,
            enabled: self.enabled.clone(),
        }))
    }
}

impl CredentialTransaction for FailingTransaction {
    fn load(&mut self) -> Result<StoredAuth> {
        self.inner.load()
    }

    fn save(&mut self, state: &StoredAuth) -> Result<()> {
        let replacing = state.registrations.iter().any(|registration| {
            registration
                .credentials
                .as_ref()
                .and_then(|credentials| credentials.access_token.as_ref())
                .is_some_and(|token| token.expose_secret() == "access-next")
        });
        ensure!(
            !replacing || !self.enabled.load(Ordering::Acquire),
            "injected replacement write failure"
        );
        self.inner.save(state)
    }
}

#[tokio::test]
async fn failed_replacement_cannot_replay_rotated_token_after_restart() -> Result<()> {
    let fixture = Fixture::new().await?;
    let status = fixture.login("account-a", SCOPES).await?;
    fixture.expire().await?;
    let failing = test_engine(
        &fixture.server,
        Arc::new(FailingStore {
            inner: fixture.store.clone(),
            enabled: Arc::new(AtomicBool::new(true)),
        }),
    )?;
    let _token = fixture.token(200, json!({"access_token": "access-next", "refresh_token": "refresh-next", "expires_in": 3600, "token_type": "Bearer"})).await;
    assert!(
        failing
            .source(&status.id)
            .access_token(CancellationToken::new())
            .await
            .is_err()
    );
    assert!(
        fixture
            .state()
            .await?
            .registrations
            .first()
            .context("registration missing")?
            .refresh_pending
    );
    assert!(
        fixture
            .engine
            .source(&status.id)
            .access_token(CancellationToken::new())
            .await
            .is_err()
    );
    assert_eq!(fixture.token_forms().await?.len(), 2);
    Ok(())
}
