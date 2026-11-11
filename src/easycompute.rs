//! Tsinghua EasyCompute and its Paratera control-plane backend.
//!
//! EasyCompute authenticates through Tsinghua OAuth and returns a short-lived
//! `access_code`.  That code is exchanged privately for the Paratera session
//! token used by the machine and API-key APIs.  Both values stay in a private
//! mode-600 sidecar and are never included in command output.

use anyhow::{anyhow, bail, Context, Result};
use md5::{Digest, Md5};
use reqwest::{Client, Method, Response};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};
use url::Url;

use crate::{
    auth::{self, LoginOptions},
    credential,
    session::Session,
};

const EASYCOMPUTE_ROOT: &str = "https://easycompute.cs.tsinghua.edu.cn";
const EASYCOMPUTE_LOGIN: &str = "https://easycompute.cs.tsinghua.edu.cn/auth/login";
const PARATERA_ROOT: &str = "https://ai.paratera.com";
const AUTH_SIDECAR: &str = "easycompute-auth";

fn operation(
    name: &str,
    mutating: bool,
    description: &str,
    required: &[&str],
    optional: &[&str],
    upstream: &str,
) -> Value {
    json!({
        "name": name,
        "mutating": mutating,
        "description": description,
        "required": required,
        "optional": optional,
        "upstream": upstream
    })
}

pub fn operations() -> Value {
    json!([
        operation(
            "easycompute.account",
            false,
            "Current EasyCompute account, coupon balance, access-code readiness and cluster allocations",
            &[],
            &[],
            "easycompute.cs.tsinghua.edu.cn/auth/userInfo/query"
        ),
        operation(
            "easycompute.clusters",
            false,
            "Current EasyCompute/Paratera clusters and zones used by machine creation",
            &[],
            &[],
            "ai.paratera.com/platform/zone/getZoneList"
        ),
        operation(
            "paratera.session",
            false,
            "Exchange the EasyCompute Tsinghua code for a private Paratera session and return the account",
            &[],
            &[],
            "ai.paratera.com/platform/tsinghua/authAndGetCoupon"
        ),
        operation(
            "paratera.access-keys",
            false,
            "List Paratera API access-key metadata with secrets redacted",
            &[],
            &["page_num", "page_size", "account_name", "payload"],
            "ai.paratera.com/platform/cipher/queryCiphers"
        ),
        operation(
            "paratera.access-key.create",
            true,
            "Create a Paratera API key and save its full response to a new private file",
            &["output"],
            &["account_name"],
            "ai.paratera.com/platform/cipher/createCipher"
        ),
        operation(
            "paratera.access-key.status",
            true,
            "Enable or disable one Paratera API key by ID",
            &["id", "enabled"],
            &[],
            "ai.paratera.com/platform/cipher/updateCipher"
        ),
        operation(
            "paratera.access-key.delete",
            true,
            "Delete one Paratera API key by ID",
            &["id"],
            &[],
            "ai.paratera.com/platform/cipher/deleteCipher"
        ),
        operation(
            "paratera.containers",
            false,
            "List Paratera container instances",
            &[],
            &["payload"],
            "ai.paratera.com/platform/ack/service/getUserResourceContainerServiceDetailList"
        ),
        operation(
            "paratera.api-keys",
            false,
            "List LLM-platform API-key metadata without exposing key material",
            &[],
            &["page_num", "page_size"],
            "ai.paratera.com/lms/api/key/llmSk"
        ),
        operation(
            "paratera.api-key.create",
            true,
            "Create one LLM-platform API key and save the one-time secret response to a private file",
            &["token_name", "output"],
            &["billing_type"],
            "ai.paratera.com/lms/api/key/create"
        ),
        operation(
            "paratera.api-key.rename",
            true,
            "Rename one LLM-platform API key by token UUID",
            &["token_uuid", "token_name"],
            &[],
            "ai.paratera.com/lms/api/key/update"
        ),
        operation(
            "paratera.api-key.status",
            true,
            "Enable or disable one LLM-platform API key by token UUID",
            &["token_uuid", "enabled"],
            &[],
            "ai.paratera.com/lms/api/key/update"
        ),
        operation(
            "paratera.api-key.delete",
            true,
            "Delete one LLM-platform API key by token UUID",
            &["token_uuid"],
            &[],
            "ai.paratera.com/lms/api/key/delete"
        ),
        operation(
            "paratera.container.options",
            false,
            "Read zones, container offerings, specs, public image types, and private images without creating a machine",
            &[],
            &["spec_payload", "image_payload", "availability_payload"],
            "ai.paratera.com/platform resource/container APIs"
        ),
        operation(
            "paratera.container.preflight",
            false,
            "Check container price and current availability for an exact server-shaped configuration",
            &["payload"],
            &[],
            "resourcePrice/checkContainerPrice + resourceContainerService/describeAvailableResource"
        ),
        operation(
            "paratera.container.plan",
            false,
            "Build a validated container-create payload from explicit resource choices without creating it",
            &[
                "alias_name",
                "cluster_id",
                "region_id",
                "zone_id",
                "service_model",
                "image_uuid"
            ],
            &[
                "billing_type",
                "count",
                "volumes",
                "auto_continue",
                "pay_period"
            ],
            "local EasyCompute container-create contract"
        ),
        operation(
            "paratera.container.create",
            true,
            "Create one or more Paratera container machines from a fully reviewed server-shaped payload",
            &["payload"],
            &[],
            "ai.paratera.com/platform/ack/service/create"
        ),
        operation(
            "paratera.read",
            false,
            "Call one allowlisted read-only Paratera frontend endpoint; write endpoints are refused",
            &["base", "path"],
            &["method", "query", "body"],
            "ai.paratera.com allowlisted read API catalogue"
        )
    ])
}

pub fn execute_offline(operation: &str, args: &Value) -> Option<Result<Value>> {
    (operation == "paratera.container.plan").then(|| container_plan(args))
}

pub async fn execute(session: &Session, operation: &str, args: &Value) -> Result<Value> {
    if let Some(result) = execute_offline(operation, args) {
        return result;
    }
    match operation {
        "easycompute.account" => {
            let (api, mut stored) = EasyComputeApi::open(session).await?;
            let user = api.user().await?;
            stored.access_code = user["access_code"].as_str().unwrap_or_default().to_owned();
            save_auth(session, &stored)?;
            Ok(json!({
                "authenticated": true,
                "account": normalize_easy_user(&user),
                "access_code_ready": !stored.access_code.is_empty(),
                "session_storage": "private mode-600 sidecar"
            }))
        }
        "easycompute.clusters" => {
            let mut api = ParateraApi::open(session).await?;
            let zones = api.get("platform", "zone/getZoneList", &json!({})).await?;
            let mut clusters = BTreeMap::<i64, String>::new();
            for row in zones["data"].as_array().into_iter().flatten() {
                if let Some(id) = row["clusterId"].as_i64() {
                    let name = row["clusterName"]
                        .as_str()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .unwrap_or("unnamed");
                    clusters.entry(id).or_insert_with(|| name.to_owned());
                }
            }
            Ok(json!({
                "clusters": clusters
                    .into_iter()
                    .map(|(id, name)| json!({"id": id, "name": name}))
                    .collect::<Vec<_>>(),
                "zones": mapped_rows(&zones, concise_zone)
            }))
        }
        operation if operation.starts_with("paratera.") => {
            let mut api = ParateraApi::open(session).await?;
            match operation {
                "paratera.session" => {
                    api.probe().await?;
                    Ok(json!({
                        "authenticated": true,
                        "account": sanitize(&api.stored.account),
                        "session_storage": "private mode-600 sidecar"
                    }))
                }
                "paratera.access-keys" => access_keys(&mut api, args).await,
                "paratera.api-keys" => api_keys(&mut api, args).await,
                "paratera.api-key.create" => create_api_key(&mut api, args).await,
                "paratera.api-key.rename" => {
                    let token_uuid = required_string(args, "token_uuid")?;
                    let token_name = required_string(args, "token_name")?;
                    let value = api
                        .post(
                            "lms",
                            "api/key/update",
                            &[],
                            Some(&json!({
                                "tokenUuid": token_uuid,
                                "tokenName": token_name
                            })),
                        )
                        .await?;
                    Ok(json!({
                        "ok": true,
                        "token_uuid": token_uuid,
                        "token_name": token_name,
                        "provider_response": sanitize(&value)
                    }))
                }
                "paratera.api-key.status" => {
                    let token_uuid = required_string(args, "token_uuid")?;
                    let enabled = args["enabled"]
                        .as_bool()
                        .ok_or_else(|| anyhow!("enabled must be a JSON boolean"))?;
                    let value = api
                        .post(
                            "lms",
                            "api/key/update",
                            &[],
                            Some(&json!({
                                "tokenUuid": token_uuid,
                                "tokenStatus": if enabled { "valid" } else { "invalid" }
                            })),
                        )
                        .await?;
                    Ok(json!({
                        "ok": true,
                        "token_uuid": token_uuid,
                        "enabled": enabled,
                        "provider_response": sanitize(&value)
                    }))
                }
                "paratera.api-key.delete" => {
                    let token_uuid = required_string(args, "token_uuid")?;
                    let value = api
                        .request(
                            "lms",
                            Method::DELETE,
                            "api/key/delete",
                            &json!({}),
                            Some(&json!({"tokenUuid": token_uuid})),
                        )
                        .await?;
                    Ok(json!({
                        "ok": true,
                        "token_uuid": token_uuid,
                        "provider_response": sanitize(&value)
                    }))
                }
                "paratera.access-key.create" => create_access_key(&mut api, args).await,
                "paratera.access-key.status" => {
                    let id = required_string(args, "id")?;
                    let enabled = args["enabled"]
                        .as_bool()
                        .ok_or_else(|| anyhow!("enabled must be a JSON boolean"))?;
                    let value = api
                        .post(
                            "platform",
                            "cipher/updateCipher",
                            &[],
                            Some(&json!({"id": id, "cipherStatus": if enabled {1} else {0}})),
                        )
                        .await?;
                    Ok(json!({
                        "ok": true,
                        "id": id,
                        "enabled": enabled,
                        "provider_response": sanitize(&value)
                    }))
                }
                "paratera.access-key.delete" => {
                    let id = required_string(args, "id")?;
                    let value = api
                        .post(
                            "platform",
                            "cipher/deleteCipher",
                            &[],
                            Some(&json!({"id": id})),
                        )
                        .await?;
                    Ok(json!({"ok": true, "id": id, "provider_response": sanitize(&value)}))
                }
                "paratera.containers" => {
                    let payload = object_or_default(
                        args,
                        "payload",
                        json!({
                            "pageNum": 1,
                            "pageSize": 20
                        }),
                    );
                    let value = api
                        .post(
                            "platform",
                            "ack/service/getUserResourceContainerServiceDetailList",
                            &[],
                            Some(&payload),
                        )
                        .await?;
                    Ok(sanitize(&value))
                }
                "paratera.container.options" => container_options(&mut api, args).await,
                "paratera.container.preflight" => {
                    let payload = required_object(args, "payload")?;
                    validate_container_create(payload)?;
                    let mut price_payload = json!({
                        "clusterId": payload["clusterId"],
                        "regionId": payload["regionId"],
                        "zoneId": payload["zoneId"],
                        "billingType": payload["billingType"],
                        "containerService": [{"resourceModel": payload["serviceModel"]}],
                        "count": payload["count"].as_u64().unwrap_or(1)
                    });
                    if payload["billingType"] == "PrePaid" {
                        price_payload["payPeriod"] = payload["payPeriod"].clone();
                    }
                    if payload["volumes"].is_array() {
                        price_payload["volumes"] = payload["volumes"].clone();
                    }
                    let availability_payload = json!([{
                        "clusterId": payload["clusterId"],
                        "regionId": payload["regionId"],
                        "zoneId": payload["zoneId"],
                        "resourceModels": [payload["serviceModel"]]
                    }]);
                    let price = api
                        .post(
                            "platform",
                            "resourcePrice/checkContainerPrice",
                            &[],
                            Some(&price_payload),
                        )
                        .await;
                    let availability = api
                        .post(
                            "platform",
                            "resourceContainerService/describeServiceAvailableResource",
                            &[],
                            Some(&availability_payload),
                        )
                        .await;
                    Ok(json!({
                        "price": result_or_error(price),
                        "availability": result_or_error(availability),
                        "configuration": payload,
                        "price_request": price_payload,
                        "availability_request": availability_payload
                    }))
                }
                "paratera.container.create" => {
                    let payload = required_object(args, "payload")?;
                    validate_container_create(payload)?;
                    let value = api
                        .post("platform", "ack/service/create", &[], Some(payload))
                        .await?;
                    Ok(sanitize(&value))
                }
                "paratera.read" => paratera_read(&mut api, args).await,
                _ => bail!("unknown EasyCompute/Paratera operation: {operation}"),
            }
        }
        _ => bail!("unknown EasyCompute/Paratera operation: {operation}"),
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
struct StoredAuth {
    #[serde(default)]
    easy_token: String,
    #[serde(default)]
    access_code: String,
    #[serde(default)]
    paratera_token: String,
    #[serde(default)]
    account: Value,
}

fn load_auth(session: &Session) -> Result<StoredAuth> {
    let Some(raw) = session.read_private_sidecar(AUTH_SIDECAR)? else {
        return Ok(StoredAuth::default());
    };
    serde_json::from_str(&raw)
        .context("EasyCompute private session metadata is malformed; run login again")
}

fn save_auth(session: &Session, auth: &StoredAuth) -> Result<()> {
    session.write_private_sidecar(
        AUTH_SIDECAR,
        &serde_json::to_string(auth).context("serializing EasyCompute session metadata")?,
    )
}

struct EasyComputeApi {
    client: Client,
    token: String,
}

impl EasyComputeApi {
    async fn open(session: &Session) -> Result<(Self, StoredAuth)> {
        let mut stored = load_auth(session)?;
        if !stored.easy_token.is_empty() {
            let api = Self {
                client: session.client.clone(),
                token: stored.easy_token.clone(),
            };
            match api.user().await {
                Ok(user) if user["cardNo"].is_string() => {
                    stored.access_code =
                        user["access_code"].as_str().unwrap_or_default().to_owned();
                    save_auth(session, &stored)?;
                    return Ok((api, stored));
                }
                _ => {}
            }
        }

        let profile = session.load_profile()?.ok_or_else(|| {
            anyhow!("Info login metadata is missing; log in once with --trust-device")
        })?;
        let username = profile.username;
        let password = credential::get(&session.credential_path(), &username)?
            .map(|secret| secret.to_string())
            .unwrap_or_default();
        let options = LoginOptions {
            username: username.clone(),
            password,
            fingerprint: profile.fingerprint,
            service: "info".into(),
            two_factor_method: None,
            interactive: false,
            trust_device: true,
            use_webvpn: false,
        };
        let token = easycompute_login(session, &options).await?;
        let api = Self {
            client: session.client.clone(),
            token: token.clone(),
        };
        let user = api.user().await?;
        if user["account"].as_str() != Some(username.as_str())
            && user["cardNo"].as_str() != Some(username.as_str())
        {
            bail!("EasyCompute session belongs to a different identity");
        }
        stored.easy_token = token;
        stored.access_code = user["access_code"].as_str().unwrap_or_default().to_owned();
        save_auth(session, &stored)?;
        Ok((api, stored))
    }

    async fn get(&self, path: &str) -> Result<Value> {
        self.request(Method::GET, path, None).await
    }

    async fn request(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value> {
        validate_relative_path(path)?;
        let mut url = Url::parse(EASYCOMPUTE_ROOT)?;
        url.set_path(path);
        let mut request = self
            .client
            .request(method, url)
            .header("token", &self.token)
            .header("Accept", "application/json");
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await.context("EasyCompute request failed")?;
        checked_response(response, "EasyCompute").await
    }

    async fn user(&self) -> Result<Value> {
        let value = self.get("/auth/userInfo/query").await?;
        let data = value
            .get("data")
            .cloned()
            .filter(|value| value.is_object())
            .ok_or_else(|| anyhow!("EasyCompute user response contained no account data"))?;
        Ok(data)
    }
}

async fn easycompute_login(session: &Session, options: &LoginOptions) -> Result<String> {
    let response = session
        .auth_client
        .get(EASYCOMPUTE_LOGIN)
        .send()
        .await
        .context("starting EasyCompute OAuth")?
        .error_for_status()
        .context("EasyCompute OAuth returned an HTTP error")?;
    let entry = response
        .text()
        .await
        .context("reading EasyCompute OAuth response")?;
    let callback = auth::destination_login(&session.auth_client, &entry, options).await?;
    let response = session
        .auth_client
        .get(callback)
        .send()
        .await
        .context("completing EasyCompute OAuth")?
        .error_for_status()
        .context("EasyCompute OAuth callback returned an HTTP error")?;
    let callback_url = response.url().clone();
    let token = callback_url
        .query_pairs()
        .find(|(key, _)| key == "token")
        .map(|(_, value)| value.into_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("EasyCompute OAuth callback did not provide a frontend token"))?;
    Ok(token)
}

fn normalize_easy_user(user: &Value) -> Value {
    let mut out = sanitize(user);
    if let Some(map) = out.as_object_mut() {
        for key in [
            "access_code",
            "token",
            "uuid",
            "id_card",
            "idCard",
            "mphone",
            "phone",
        ] {
            map.remove(key);
        }
        if let Some(class) = user["clazz"].as_str() {
            map.insert(
                "identity_class".into(),
                json!(match class {
                    "X0031" => "本科生",
                    "X0021" => "研究生",
                    "X0011" => "博士生",
                    "X0041" => "进修生",
                    "J0000" => "在职教工",
                    "H0000" => "非事业编制人事管理人员",
                    _ => "unknown",
                }),
            );
        }
    }
    out
}

struct ParateraApi {
    client: Client,
    token: String,
    stored: StoredAuth,
}

impl ParateraApi {
    async fn open(session: &Session) -> Result<Self> {
        let (easy, mut stored) = EasyComputeApi::open(session).await?;
        let user = easy.user().await?;
        let access_code = user["access_code"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("EasyCompute account has no Paratera access code"))?
            .to_owned();
        stored.easy_token = easy.token.clone();
        stored.access_code = access_code.clone();

        let client = session.client.clone();
        if !stored.paratera_token.is_empty() {
            let mut api = Self {
                client: client.clone(),
                token: stored.paratera_token.clone(),
                stored,
            };
            if api.probe().await.is_ok() && api.stored.account.is_object() {
                save_auth(session, &api.stored)?;
                return Ok(api);
            }
            stored = api.stored;
        }

        let mut url = Url::parse(&format!(
            "{PARATERA_ROOT}/platform/tsinghua/authAndGetCoupon"
        ))?;
        url.query_pairs_mut()
            .append_pair("accessKey", "paraterA")
            .append_pair("accessCode", &access_code);
        let response = client
            .get(url)
            .send()
            .await
            .context("Paratera Tsinghua authorization failed")?;
        let status = response.status();
        let token = response
            .headers()
            .get("token")
            .or_else(|| response.headers().get("para_token"))
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
            .filter(|value| !value.is_empty());
        let body = response
            .text()
            .await
            .context("reading Paratera authorization response")?;
        if !status.is_success() {
            bail!("Paratera authorization returned HTTP {status}");
        }
        let value: Value = serde_json::from_str(&body)
            .context("Paratera authorization returned malformed JSON")?;
        if !provider_ok(&value) {
            bail!("Paratera rejected the Tsinghua access code");
        }
        let token = token.ok_or_else(|| {
            anyhow!("Paratera authorization did not return a private session token")
        })?;
        stored.paratera_token = token.clone();
        stored.account = value["data"].clone();
        if !stored.account.is_object() {
            stored.account = Value::Null;
        }
        save_auth(session, &stored)?;
        Ok(Self {
            client,
            token,
            stored,
        })
    }

    async fn probe(&mut self) -> Result<()> {
        let value = self.get("platform", "zone/getZoneList", &json!({})).await?;
        if !provider_ok(&value) {
            bail!("Paratera rejected the private session")
        }
        Ok(())
    }

    async fn get(&mut self, base: &str, path: &str, query: &Value) -> Result<Value> {
        self.request(base, Method::GET, path, query, None).await
    }

    async fn post(
        &mut self,
        base: &str,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> Result<Value> {
        let query = query
            .iter()
            .map(|(key, value)| ((*key).to_owned(), json!(value)))
            .collect::<serde_json::Map<_, _>>();
        self.request(base, Method::POST, path, &Value::Object(query), body)
            .await
    }

    async fn request(
        &mut self,
        base: &str,
        method: Method,
        path: &str,
        query: &Value,
        body: Option<&Value>,
    ) -> Result<Value> {
        validate_provider_base(base)?;
        validate_relative_path(path)?;
        let mut url = Url::parse(&format!("{PARATERA_ROOT}/{base}/"))?;
        let joined = url.join(path)?;
        url = joined;
        append_query(&mut url, query)?;
        let digest = Md5::digest(self.token.as_bytes());
        let mut request = self
            .client
            .request(method, url)
            .header("token", &self.token)
            .header("Ai-Authorization", format!("Bearer {digest:x}"))
            .header("Accept", "application/json");
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await.context("Paratera request failed")?;
        if let Some(token) = response
            .headers()
            .get("token")
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty())
        {
            self.token = token.to_owned();
            self.stored.paratera_token = token.to_owned();
        }
        let value = checked_response(response, "Paratera").await?;
        Ok(value)
    }
}

fn validate_container_create(payload: &Value) -> Result<()> {
    let object = payload
        .as_object()
        .ok_or_else(|| anyhow!("container payload must be a JSON object"))?;
    if object.is_empty() {
        bail!("container payload must not be empty");
    }
    for key in ["aliasName", "serviceModel", "imageUuid", "billingType"] {
        if !object.get(key).is_some_and(|value| {
            value.as_str().is_some_and(|value| !value.trim().is_empty()) || value.as_i64().is_some()
        }) {
            bail!("container payload is missing a usable {key}");
        }
    }
    Ok(())
}

fn container_plan(args: &Value) -> Result<Value> {
    let alias_name = required_string(args, "alias_name")?;
    if !(8..=64).contains(&alias_name.len())
        || alias_name.starts_with(|character: char| character.is_ascii_digit() || character == '-')
        || !alias_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        bail!(
            "alias_name must be 8-64 ASCII letters, digits, or hyphens and cannot start with a digit or hyphen"
        )
    }
    let billing_type = optional_string(args, "billing_type").unwrap_or_else(|| "PostPaid".into());
    if !matches!(billing_type.as_str(), "PostPaid" | "PrePaid") {
        bail!("billing_type must be PostPaid or PrePaid")
    }
    let count = required_u64(args, "count", 1)?;
    if count > 20 {
        bail!("count must not exceed 20")
    }
    let volumes = args.get("volumes").cloned().unwrap_or_else(|| {
        json!([
            {"volumeType": "LOCAL", "volumeSize": 20},
            {"volumeType": "SHARED", "volumeSize": 100}
        ])
    });
    let volumes = volumes
        .as_array()
        .ok_or_else(|| anyhow!("volumes must be a JSON array"))?;
    for volume in volumes {
        if !matches!(
            volume["volumeType"].as_str(),
            Some("LOCAL" | "SHARED" | "CFS")
        ) || !volume["volumeSize"].as_u64().is_some_and(|size| size > 0)
        {
            bail!("each volume needs volumeType LOCAL/SHARED/CFS and a positive volumeSize")
        }
    }
    let mut payload = json!({
        "count": count,
        "aliasName": alias_name,
        "clusterId": required_u64(args, "cluster_id", 0)?,
        "regionId": required_u64(args, "region_id", 0)?,
        "zoneId": required_u64(args, "zone_id", 0)?,
        "billingType": billing_type,
        "serviceModel": required_string(args, "service_model")?,
        "imageUuid": required_string(args, "image_uuid")?,
        "autoContinue": args["auto_continue"].as_bool().unwrap_or(false),
        "volumes": volumes
    });
    if payload["clusterId"].as_u64() == Some(0)
        || payload["regionId"].as_u64() == Some(0)
        || payload["zoneId"].as_u64() == Some(0)
    {
        bail!("cluster_id, region_id, and zone_id must be positive integers")
    }
    if payload["billingType"] == "PostPaid" {
        if let Some(object) = payload.as_object_mut() {
            object.remove("payPeriod");
        }
    } else {
        payload["payPeriod"] = json!(required_u64(args, "pay_period", 1)?);
    }
    validate_container_create(&payload)?;
    Ok(json!({
        "network_request_sent": false,
        "payload": payload,
        "next": [
            "Review the exact cluster, zone, model, image, volume, and billing values.",
            "Run paratera.container.preflight with this payload object.",
            "Only after explicit approval, run paratera.container.create with --confirm."
        ]
    }))
}

fn required_u64(args: &Value, key: &str, default: u64) -> Result<u64> {
    match args.get(key) {
        None | Some(Value::Null) if default > 0 => Ok(default),
        Some(Value::Number(value)) => value
            .as_u64()
            .ok_or_else(|| anyhow!("{key} must be a positive integer")),
        Some(Value::String(value)) => value
            .parse::<u64>()
            .with_context(|| format!("{key} must be a positive integer")),
        _ => bail!("{key} must be a positive integer"),
    }
}

async fn access_keys(api: &mut ParateraApi, args: &Value) -> Result<Value> {
    let account_name = account_name(api, args).await?;
    let mut payload = object_or_default(args, "payload", json!({}));
    let object = payload
        .as_object_mut()
        .ok_or_else(|| anyhow!("payload must be a JSON object"))?;
    object
        .entry("pageNum")
        .or_insert_with(|| json!(optional_u64(args, "page_num", 1)));
    object
        .entry("pageSize")
        .or_insert_with(|| json!(optional_u64(args, "page_size", 20)));
    object
        .entry("accountName")
        .or_insert_with(|| json!(account_name));
    let value = api
        .post("platform", "cipher/queryCiphers", &[], Some(&payload))
        .await?;
    let rows = value["data"]["rows"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|row| {
            json!({
                "id": row["id"],
                "access_key": mask_secret(row["accessKey"].as_str().unwrap_or_default()),
                "created_time": row["createdTime"],
                "updated_time": row["updatedTime"],
                "enabled": row["cipherStatus"].as_i64() == Some(1),
                "secret_is_available_to_provider": !row["accessSecret"].is_null()
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "count": value["data"]["total"],
        "page_num": value["data"]["pageNum"],
        "access_keys": rows
    }))
}

async fn api_keys(api: &mut ParateraApi, args: &Value) -> Result<Value> {
    let payload = json!({
        "pageNum": optional_u64(args, "page_num", 1),
        "pageSize": optional_u64(args, "page_size", 20)
    });
    let value = api
        .post("lms", "api/key/llmSk", &[], Some(&payload))
        .await?;
    let rows = value["data"]["rows"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|row| {
            json!({
                "id": row["id"],
                "token_uuid": row["tokenUuid"],
                "name": row["tokenName"],
                "status": row["tokenStatus"],
                "billing_type": row["billingType"],
                "models": row["modelScale"],
                "created_time": row["createTime"],
                "updated_time": row["updateTime"]
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "count": value["data"]["total"],
        "page_num": value["data"]["pageNum"],
        "api_keys": rows
    }))
}

async fn create_api_key(api: &mut ParateraApi, args: &Value) -> Result<Value> {
    let output = required_string(args, "output")?;
    let token_name = required_string(args, "token_name")?;
    let billing_type = optional_string(args, "billing_type").unwrap_or_else(|| "PostPaid".into());
    if !matches!(billing_type.as_str(), "PostPaid" | "PrePaid") {
        bail!("billing_type must be PostPaid or PrePaid")
    }
    let value = api
        .post(
            "lms",
            "api/key/create",
            &[],
            Some(&json!({
                "tokenName": token_name,
                "billingType": billing_type
            })),
        )
        .await?;
    write_private_output(Path::new(&output), &value)?;
    Ok(json!({
        "ok": true,
        "written_to": absolute_display_path(Path::new(&output)),
        "permissions": "0600",
        "note": "The provider response contains the one-time API key and was written without printing it."
    }))
}

async fn create_access_key(api: &mut ParateraApi, args: &Value) -> Result<Value> {
    let output = required_string(args, "output")?;
    let account_name = account_name(api, args).await?;
    let value = api
        .post(
            "platform",
            "cipher/createCipher",
            &[],
            Some(&json!({"accountName": account_name})),
        )
        .await?;
    write_private_output(Path::new(&output), &value)?;
    Ok(json!({
        "ok": true,
        "written_to": absolute_display_path(Path::new(&output)),
        "permissions": "0600",
        "note": "The provider response was written without printing API-key material."
    }))
}

async fn account_name(api: &mut ParateraApi, args: &Value) -> Result<String> {
    if let Some(value) = optional_string(args, "account_name") {
        return Ok(value);
    }
    let account = &api.stored.account;
    for key in ["email", "mphone", "userAccount", "account", "loginName"] {
        if let Some(value) = account[key].as_str().filter(|value| !value.is_empty()) {
            return Ok(value.to_owned());
        }
    }
    bail!("Paratera account has no usable email, phone, or account name")
}

async fn container_options(api: &mut ParateraApi, args: &Value) -> Result<Value> {
    let mut errors = serde_json::Map::new();
    let zones = api.get("platform", "zone/getZoneList", &json!({})).await;
    let offerings = api
        .get(
            "platform",
            "resourceContainerService/getResourceContainerServiceList",
            &json!({}),
        )
        .await;
    let resource_types = api
        .post(
            "platform",
            "resourceContainerService/getUserResourceList",
            &[],
            None,
        )
        .await;
    let specs = api
        .get(
            "platform",
            "resourceContainerService/getServiceResourceSpecList",
            args.get("spec_payload")
                .filter(|value| value.is_object())
                .unwrap_or(&json!({"resourceType": "ACKCS"})),
        )
        .await;
    let user_images = api
        .post(
            "platform",
            "ack/image/getUserImageList",
            &[],
            Some(&json!({"pageNum": 1, "pageSize": 999})),
        )
        .await;
    let image_types = api
        .get(
            "platform",
            "resourceContainerImage/getResourceImage",
            &json!({}),
        )
        .await;
    let images = if args["image_payload"].is_object() {
        api.post(
            "platform",
            "resourceContainerImage/getResourceCommonImage",
            &[],
            Some(&args["image_payload"]),
        )
        .await
    } else {
        Ok(Value::Null)
    };
    let availability = if args["availability_payload"].is_object() {
        api.post(
            "platform",
            "resourceContainerService/describeAvailableResource",
            &[],
            Some(&args["availability_payload"]),
        )
        .await
    } else {
        Ok(Value::Null)
    };
    let zones = result_or_error_with_key(zones, "zones", &mut errors);
    let offerings = result_or_error_with_key(offerings, "offerings", &mut errors);
    let resource_types = result_or_error_with_key(resource_types, "resource_types", &mut errors);
    let specs = result_or_error_with_key(specs, "specs", &mut errors);
    let user_images = result_or_error_with_key(user_images, "user_images", &mut errors);
    let image_types = result_or_error_with_key(image_types, "image_types", &mut errors);
    let images = result_or_error_with_key(images, "common_images", &mut errors);
    Ok(json!({
        "zones": mapped_rows(&zones, concise_zone),
        "offerings": mapped_rows(&offerings, concise_offering),
        "resource_types": resource_types,
        "specs": specs,
        "user_images": user_images,
        "image_types": image_types,
        "common_images": images,
        "availability": result_or_error_with_key(availability, "availability", &mut errors),
        "errors": errors
    }))
}

fn mapped_rows(value: &Value, mapper: fn(&Value) -> Value) -> Value {
    value["data"]
        .as_array()
        .map(|rows| Value::Array(rows.iter().map(mapper).collect()))
        .unwrap_or(Value::Null)
}

fn concise_zone(row: &Value) -> Value {
    json!({
        "id": row["id"],
        "name": row["zoneAlias"].as_str().or_else(|| row["zoneName"].as_str()),
        "zone_code": row["zoneCode"],
        "cluster_id": row["clusterId"],
        "cluster_name": row["clusterName"],
        "region_id": row["regionId"],
        "region_name": row["regionName"],
        "status": row["status"]
    })
}

fn concise_offering(row: &Value) -> Value {
    json!({
        "id": row["id"],
        "service_model": row["serviceModel"],
        "service_type": row["serviceType"],
        "service_type_name": row["serviceTypeAlias"],
        "gpu_count": row["serviceGpus"],
        "gpu_memory_bytes": row["serviceGpumem"],
        "cpu_count": row["serviceCpus"],
        "memory_bytes": row["serviceMemory"],
        "system_disk_gb": row["localSysdiskSize"],
        "cluster_id": row["clusterId"],
        "cluster_name": row["clusterName"],
        "region_id": row["regionId"],
        "region_name": row["regionName"],
        "zone_id": row["zoneId"],
        "zone_name": row["zoneName"],
        "postpaid_price": row["strategies"]["PostPaid"]["unitPrice"],
        "prepaid_price": row["strategies"]["PrePaid"]["unitPrice"],
        "product_status": row["productStatus"],
        "service_status": row["serviceStatus"]
    })
}

async fn paratera_read(api: &mut ParateraApi, args: &Value) -> Result<Value> {
    let base = required_string(args, "base")?;
    let path = required_string(args, "path")?;
    let method = optional_string(args, "method")
        .unwrap_or_else(|| "GET".into())
        .to_ascii_uppercase();
    if !read_request_allowed(&base, &path, &method) {
        bail!("This Paratera endpoint/method is not on the read-only allowlist");
    }
    let default_query = json!({});
    let query = args.get("query").unwrap_or(&default_query);
    let body = args.get("body").filter(|value| value.is_object());
    let method = Method::from_bytes(method.as_bytes()).context("invalid HTTP method")?;
    let value = api.request(&base, method, &path, query, body).await?;
    Ok(sanitize(&value))
}

fn read_request_allowed(base: &str, path: &str, method: &str) -> bool {
    let path = path.trim_start_matches('/');
    if method == "GET" {
        return match base {
            "platform" => [
                "zone/",
                "region/",
                "cluster/",
                "resource",
                "userResource",
                "statement/",
                "billingBalanceDetail/",
                "config/",
                "paraDataset/",
            ]
            .iter()
            .any(|prefix| path.starts_with(prefix)),
            "osswxservice" => [
                "ack/image/getList",
                "ack/service/describe/",
                "ack/service/port/describe",
                "ack/resourceContainerVolumeBasic/getList",
            ]
            .iter()
            .any(|prefix| path.starts_with(prefix)),
            "lms" => path.starts_with("api/key/"),
            "message" | "aimonitor" | "aistudio" => true,
            _ => false,
        };
    }
    if method != "POST" {
        return false;
    }
    let read_prefixes = [
        "zone/DescribeZones",
        "zone/getList",
        "resourceVhost/get",
        "resourceDisk/get",
        "resourceNetwork/get",
        "resourceContainerService/",
        "resourceContainerImage/get",
        "resourceContainerVolume/get",
        "resourcePrice/check",
        "resourceOverview/get",
        "userResourceVhost/get",
        "userResourceDisk/get",
        "userResourceNetwork/get",
        "userResourceSnap/get",
        "userResourceImage/get",
        "statement/get",
        "billingBalanceDetail/get",
        "config/getList",
        "paraDataset/user/list",
        "cipher/queryCiphers",
        "jobs/results",
        "jobs/ack",
        "ack/service/getUserResourceContainerServiceDetailList",
        "ack/image/getList",
        "ack/image/getUserImageList",
        "ack/resourceContainerVolumeBasic/getList",
        "ack/volume/getUserResourceContainerVolumeDetailList",
        "ack/volume/transfer/describe",
        "ack/pool/getList",
        "task/getUserResourceAistudioList",
    ];
    read_prefixes.iter().any(|prefix| path.starts_with(prefix))
        || (base == "lms" && path.starts_with("api/key/"))
}

async fn checked_response(response: Response, service: &str) -> Result<Value> {
    let status = response.status();
    let text = response
        .text()
        .await
        .with_context(|| format!("reading {service} response"))?;
    if !status.is_success() {
        let provider_message = serde_json::from_str::<Value>(&text).ok().and_then(|value| {
            value["message"]
                .as_str()
                .or_else(|| value["msg"].as_str())
                .map(str::to_owned)
        });
        if matches!(status.as_u16(), 401 | 403) {
            bail!("{service} session expired");
        }
        if let Some(message) = provider_message {
            bail!("{service} returned HTTP {status}: {message}");
        }
        bail!("{service} returned HTTP {status}");
    }
    let value: Value = serde_json::from_str(&text)
        .with_context(|| format!("{service} returned malformed JSON"))?;
    if !provider_ok(&value) {
        let message = value["message"]
            .as_str()
            .or_else(|| value["msg"].as_str())
            .unwrap_or("provider rejected the request");
        if value["code"]
            .as_i64()
            .is_some_and(|code| code == 401 || code == 403)
            || value["errorCode"]
                .as_i64()
                .is_some_and(|code| matches!(code, 401 | 403 | 100005 | 100006))
        {
            bail!("{service} session expired: {message}");
        }
        bail!("{service} rejected the request: {message}");
    }
    Ok(value)
}

fn provider_ok(value: &Value) -> bool {
    value["success"].as_bool() == Some(true)
        || matches!(value["code"].as_i64(), Some(0) | Some(200))
}

fn append_query(url: &mut Url, query: &Value) -> Result<()> {
    let Some(object) = query.as_object() else {
        if query.is_null() {
            return Ok(());
        }
        bail!("query must be a JSON object");
    };
    for (key, value) in object {
        if value.is_null() {
            continue;
        }
        let value = match value {
            Value::String(value) => value.clone(),
            Value::Bool(_) | Value::Number(_) => value.to_string(),
            _ => bail!("query value for {key} must be a string, number, or boolean"),
        };
        url.query_pairs_mut().append_pair(key, &value);
    }
    Ok(())
}

fn validate_provider_base(base: &str) -> Result<()> {
    if matches!(
        base,
        "platform" | "lms" | "osswxservice" | "aistudio" | "message" | "aimonitor"
    ) {
        Ok(())
    } else {
        bail!("unsupported Paratera API base")
    }
}

fn validate_relative_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.starts_with("http://")
        || path.starts_with("https://")
        || path.contains('?')
        || path.contains('#')
        || path.split('/').any(|part| part == "..")
    {
        bail!("unsafe or malformed provider API path");
    }
    Ok(())
}

fn required_string(args: &Value, key: &str) -> Result<String> {
    args[key]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("missing nonempty string argument: {key}"))
}

fn optional_string(args: &Value, key: &str) -> Option<String> {
    args[key]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn required_object<'a>(args: &'a Value, key: &str) -> Result<&'a Value> {
    args.get(key)
        .filter(|value| value.is_object())
        .ok_or_else(|| anyhow!("missing JSON object argument: {key}"))
}

fn object_or_default(args: &Value, key: &str, default: Value) -> Value {
    args.get(key)
        .filter(|value| value.is_object())
        .cloned()
        .unwrap_or(default)
}

fn optional_u64(args: &Value, key: &str, default: u64) -> u64 {
    args[key].as_u64().unwrap_or(default)
}

fn result_or_error<T>(result: Result<T>) -> Value
where
    T: Into<Value>,
{
    match result {
        Ok(value) => sanitize(&value.into()),
        Err(error) => json!({"error": error.to_string()}),
    }
}

fn result_or_error_with_key(
    result: Result<Value>,
    key: &str,
    errors: &mut serde_json::Map<String, Value>,
) -> Value {
    match result {
        Ok(value) => sanitize(&value),
        Err(error) => {
            errors.insert(key.to_owned(), json!(error.to_string()));
            Value::Null
        }
    }
}

fn sanitize(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    let lower = key.to_ascii_lowercase();
                    if [
                        "token",
                        "secret",
                        "password",
                        "access_code",
                        "accesscode",
                        "accesssecret",
                    ]
                    .iter()
                    .any(|needle| lower.contains(needle))
                    {
                        (key.clone(), json!("[redacted]"))
                    } else {
                        (key.clone(), sanitize(value))
                    }
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(sanitize).collect()),
        _ => value.clone(),
    }
}

fn mask_secret(value: &str) -> String {
    if value.is_empty() {
        return "[redacted]".into();
    }
    let suffix = value
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("****{suffix}")
}

fn write_private_output(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        if !parent.exists() {
            bail!("output directory does not exist");
        }
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).with_context(|| {
        format!(
            "cannot create private output {}",
            absolute_display_path(path)
        )
    })?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn absolute_display_path(path: &Path) -> String {
    if path.is_absolute() {
        path.display().to_string()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path).display().to_string())
            .unwrap_or_else(|_| path.display().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_nested_secret_fields() {
        let value = json!({
            "access_code": "secret",
            "nested": {"accessSecret": "also-secret", "name": "visible"},
            "items": [{"token": "secret-token"}]
        });
        let output = sanitize(&value);
        assert_eq!(output["access_code"], "[redacted]");
        assert_eq!(output["nested"]["accessSecret"], "[redacted]");
        assert_eq!(output["nested"]["name"], "visible");
        assert_eq!(output["items"][0]["token"], "[redacted]");
    }

    #[test]
    fn dead_easycompute_cluster_routes_are_not_advertised() {
        let operations = operations();
        let names = operations
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|operation| operation["name"].as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"easycompute.clusters"));
        assert!(!names.contains(&"easycompute.cluster-options"));
        assert!(!names.contains(&"easycompute.cluster.apply"));
    }

    #[test]
    fn read_allowlist_refuses_known_writes() {
        assert!(read_request_allowed(
            "platform",
            "statement/getParaStatement",
            "POST"
        ));
        assert!(!read_request_allowed(
            "osswxservice",
            "ack/service/create",
            "POST"
        ));
        assert!(!read_request_allowed(
            "platform",
            "cipher/createCipher",
            "POST"
        ));
    }

    #[test]
    fn masks_keys_without_revealing_body() {
        assert_eq!(mask_secret("ABCDEFGHIJK"), "****HIJK");
        assert_eq!(mask_secret(""), "[redacted]");
    }
}
