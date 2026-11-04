//! Native client for the Tsinghua online reimbursement service.
use crate::session::Session;
use aes::Aes128;
use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyInit};
use rand::{rngs::OsRng, RngCore};
use reqwest::{
    header::{HeaderMap, HeaderValue},
    multipart::{Form, Part},
    Method, Url,
};
use rsa::{pkcs8::DecodePublicKey, Pkcs1v15Encrypt, RsaPublicKey};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use zeroize::Zeroizing;

const API_ROOT: &str = "https://wsbx.tsinghua.edu.cn/wsyy/api/v1/wsyy-api";
const INFO_REDIRECT: &str =
    "https://info.tsinghua.edu.cn/b/yyfw/vyyfwxx/info/portal_fg/common/onlineAppRedirect";
const WSBX_APP_ID: &str = "95E04FA9C49C5555E6ED1A642A043B9B";
const DEFAULT_YWLX: &str = "052";
const SIDECAR: &str = "wsbx-auth";
const MAX_API_RESPONSE: u64 = 32 * 1024 * 1024;
const MAX_UPLOAD_BYTES: u64 = 30 * 1024 * 1024;
const PUBLIC_KEY: &str = r#"-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAsteCb9QD4v7o7F3gHQSrR3s2EHtUgSp5b+KFOFrjtBGcWNgfLzz9Sv0zqLZno4oBJobvKQ6lvzS+drx7G2MxL/iNDh+7Cu2RZrY7ENILBKV20zKYRTeZLLImUgPZqtZBP1rUT+ig3SssdeNM628Hw65csM9ng4/q3UV80wNTHF+A4QpTJ2gppBv5fy9XiTrlJ2kTD9lNA75KNw4F6vZ7noWOMWUqyZRBm0BvYfajdfJo1uqerfLMhT/dAdEsUcFJTneSGRSt1yWYLDgIpV3EEZobaMt4UbpT/uWsS8XkukNTtEgzDLabF4zBiJTLJgIzBMT6Z5XlCuTiLTMnND3H7wIDAQAB
-----END PUBLIC KEY-----"#;

#[derive(Deserialize, Serialize)]
struct StoredAuth {
    token: String,
    user_id: String,
    user_name: String,
}

struct WsbxClient<'a> {
    session: &'a Session,
    auth: Option<StoredAuth>,
}

struct RawApiResponse {
    headers: HeaderMap,
    body: Vec<u8>,
    aes_key: Zeroizing<String>,
}

fn required(a: &Value, key: &str) -> Result<String> {
    a.get(key)
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string())
        })
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("missing nonempty argument: {key}"))
}

fn nonempty_string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| anyhow!("missing nonempty string argument: {key}"))
}

fn bool_arg(a: &Value, key: &str, default: bool) -> bool {
    a.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn delivery_option(a: &Value, key: &str, submit: bool) -> Result<String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(if submit { "0" } else { "" }.to_owned()),
        Some(Value::Bool(value)) => Ok(if *value { "1" } else { "0" }.to_owned()),
        Some(Value::String(value)) if value == "0" || value == "1" || value.is_empty() => {
            Ok(value.clone())
        }
        Some(_) => bail!("{key} must be a boolean or the string \"0\" or \"1\""),
    }
}

fn u64_arg(a: &Value, key: &str, default: u64) -> Result<u64> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Number(value)) => value
            .as_u64()
            .ok_or_else(|| anyhow!("{key} must be a nonnegative integer")),
        Some(Value::String(value)) => value
            .parse()
            .map_err(|_| anyhow!("{key} must be a nonnegative integer")),
        Some(_) => bail!("{key} must be a nonnegative integer"),
    }
}

fn invoice_list_status(a: &Value) -> Result<&'static str> {
    match a.get("status") {
        None | Some(Value::Null) => Ok("1"),
        Some(Value::String(value))
            if matches!(value.as_str(), "pending" | "unused" | "waiting" | "1") =>
        {
            Ok("1")
        }
        Some(Value::String(value))
            if matches!(
                value.as_str(),
                "used" | "reimbursed" | "history" | "completed" | "0"
            ) =>
        {
            Ok("0")
        }
        Some(_) => bail!(
            "status must be pending, unused, waiting, used, reimbursed, history, completed, 1, or 0"
        ),
    }
}

fn api_url(path: &str, header: Option<&str>) -> Result<Url> {
    let service = header.unwrap_or("wsyy");
    if service.is_empty() || !service.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        bail!("Invalid wsbx service header")
    }
    let path = path.trim_start_matches('/');
    if path.is_empty()
        || path.contains('?')
        || path.contains('#')
        || path
            .split('/')
            .any(|segment| segment == "." || segment == "..")
        || !path
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/_-.".contains(&byte))
    {
        bail!("Invalid wsbx API path")
    }
    Ok(Url::parse(&format!(
        "{API_ROOT}/api/{service}-service/{path}"
    ))?)
}

fn random_aes_key() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut bytes = [0_u8; 16];
    OsRng.fill_bytes(&mut bytes);
    bytes
        .iter()
        .map(|byte| ALPHABET[*byte as usize % ALPHABET.len()] as char)
        .collect()
}

fn aes_encrypt(plaintext: &[u8], key: &str) -> Result<String> {
    let encrypted = ecb::Encryptor::<Aes128>::new_from_slice(key.as_bytes())
        .map_err(|_| anyhow!("Invalid wsbx AES key"))?
        .encrypt_padded_vec_mut::<Pkcs7>(plaintext);
    Ok(STANDARD.encode(encrypted))
}

fn aes_decrypt(encrypted: &str, key: &str) -> Result<Zeroizing<String>> {
    let compact: String = encrypted
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let encrypted = STANDARD
        .decode(compact)
        .context("Invalid encrypted wsbx response")?;
    let plaintext = ecb::Decryptor::<Aes128>::new_from_slice(key.as_bytes())
        .map_err(|_| anyhow!("Invalid wsbx AES key"))?
        .decrypt_padded_vec_mut::<Pkcs7>(&encrypted)
        .map_err(|_| anyhow!("Invalid wsbx response padding"))?;
    let text = String::from_utf8(plaintext).context("Invalid UTF-8 in wsbx response")?;
    Ok(Zeroizing::new(text))
}

fn rsa_encrypt_key(key: &str) -> Result<String> {
    let der = PUBLIC_KEY
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect::<String>();
    let der = STANDARD
        .decode(der)
        .context("Invalid embedded wsbx public key encoding")?;
    let public =
        RsaPublicKey::from_public_key_der(&der).context("Invalid embedded wsbx public key")?;
    let encrypted = public
        .encrypt(&mut OsRng, Pkcs1v15Encrypt, key.as_bytes())
        .context("Could not encrypt wsbx request key")?;
    Ok(STANDARD.encode(encrypted))
}

fn encrypted_fields(data: &Value) -> Result<(Value, Zeroizing<String>)> {
    let key = Zeroizing::new(random_aes_key());
    let plaintext =
        Zeroizing::new(serde_json::to_vec(data).context("Could not serialize wsbx request")?);
    let encrypted = aes_encrypt(&plaintext, &key)?;
    let sec_key = rsa_encrypt_key(&key)?;
    Ok((json!({"data": encrypted, "encSecKey": sec_key}), key))
}

fn decrypt_response(body: &[u8], key: &str) -> Result<Value> {
    let text = std::str::from_utf8(body).context("wbx response is not UTF-8")?;
    let trimmed = text.trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return serde_json::from_str(trimmed).context("Invalid plain wsbx response JSON");
    }
    let encoded = if trimmed.starts_with('"') {
        serde_json::from_str::<String>(trimmed).context("Invalid quoted wsbx response")?
    } else {
        trimmed.to_owned()
    };
    let plaintext = aes_decrypt(&encoded, key)?;
    serde_json::from_str(&plaintext).context("Invalid decrypted wsbx response JSON")
}

fn envelope_code(value: &Value) -> Option<String> {
    value.get("code").map(|code| match code {
        Value::String(code) => code.clone(),
        Value::Number(code) => code.to_string(),
        _ => String::new(),
    })
}

fn is_authentication_code(code: &str) -> bool {
    matches!(code, "401" | "403" | "998")
}

fn auth_error() -> anyhow::Error {
    anyhow!("wsbx authentication expired; run an Info read-only login refresh and retry")
}

fn validate_envelope(value: Value) -> Result<Value> {
    let Some(code) = envelope_code(&value) else {
        bail!("wbx response has no status code")
    };
    if is_authentication_code(&code) {
        return Err(auth_error());
    }
    if code != "1001" {
        let message = value
            .get("msg")
            .and_then(Value::as_str)
            .unwrap_or("request rejected");
        bail!("wbx request rejected with code {code}: {message}")
    }
    Ok(value)
}

fn project_parts(project: &Value) -> Result<(&str, &str)> {
    Ok((
        nonempty_string(project, "bmbh")?,
        nonempty_string(project, "xmbh")?,
    ))
}

fn item_parts(item: &Value) -> Result<(&str, &str)> {
    Ok((
        nonempty_string(item, "jjflkmbh")?,
        nonempty_string(item, "jjflkmmc")?,
    ))
}

fn payments(a: &Value) -> Result<Map<String, Value>> {
    let object = a
        .get("payments")
        .map(|value| {
            value
                .as_object()
                .cloned()
                .ok_or_else(|| anyhow!("payments must be a JSON object"))
        })
        .transpose()?
        .unwrap_or_default();
    let allowed = ["cjkList", "gwkList", "xmzzList", "dszzList", "dgzzList"];
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        bail!("Unsupported payment list: {key}")
    }
    for key in allowed {
        if object.get(key).is_some_and(|value| !value.is_array()) {
            bail!("payments.{key} must be an array")
        }
    }
    let mut result = Map::new();
    for key in allowed {
        result.insert(
            key.to_owned(),
            object.get(key).cloned().unwrap_or_else(|| json!([])),
        );
    }
    Ok(result)
}

fn amount_cents(value: &Value) -> Result<u64> {
    let text = match value {
        Value::Null => return Ok(0),
        Value::String(value) => value.trim().to_owned(),
        Value::Number(value) => value.to_string(),
        _ => bail!("amount must be a string or number"),
    };
    if text.is_empty() {
        return Ok(0);
    }
    if text.starts_with('-') {
        bail!("negative reimbursement amounts are not accepted")
    }
    let mut parts = text.split('.');
    let whole = parts.next().unwrap_or("0");
    let fraction = parts.next().unwrap_or("");
    if parts.next().is_some() || fraction.len() > 2 {
        bail!("amount must have at most two decimal places")
    }
    let whole = if whole.is_empty() {
        0
    } else {
        whole
            .parse::<u64>()
            .map_err(|_| anyhow!("invalid amount"))?
    };
    let fraction = if fraction.is_empty() {
        0
    } else {
        format!("{fraction:0<2}")
            .parse::<u64>()
            .map_err(|_| anyhow!("invalid amount"))?
    };
    whole
        .checked_mul(100)
        .and_then(|value| value.checked_add(fraction))
        .ok_or_else(|| anyhow!("amount is too large"))
}

fn count_value(value: &Value) -> Result<u64> {
    match value {
        Value::Null => Ok(0),
        Value::Number(value) => value
            .as_u64()
            .ok_or_else(|| anyhow!("ticket count must be a nonnegative integer")),
        Value::String(value) if value.trim().is_empty() => Ok(0),
        Value::String(value) => value
            .parse()
            .map_err(|_| anyhow!("ticket count must be a nonnegative integer")),
        _ => bail!("ticket count must be a nonnegative integer"),
    }
}

fn selected_items(items: &[Value], require_any: bool) -> Result<Vec<Value>> {
    let mut selected = Vec::new();
    for item in items {
        if !item.is_object() {
            bail!("items must contain JSON objects")
        }
        let _ = item_parts(item)?;
        let electronic = amount_cents(item.get("dzpjSbje").unwrap_or(&Value::Null))?;
        let paper = amount_cents(item.get("zzpjBxje").unwrap_or(&Value::Null))?;
        if electronic == 0 && paper == 0 {
            continue;
        }
        let mut item = item.clone();
        let electronic_count = count_value(item.get("dzpjFjzs").unwrap_or(&Value::Null))?;
        let paper_count = count_value(item.get("zzpjFjzs").unwrap_or(&Value::Null))?;
        item["pzzshj"] = json!(electronic_count.saturating_add(paper_count));
        item["sbje"] = json!(format_amount(electronic.saturating_add(paper)));
        selected.push(item);
    }
    if require_any && selected.is_empty() {
        bail!("at least one item must have a nonzero reimbursement amount")
    }
    Ok(selected)
}

fn format_amount(cents: u64) -> String {
    format!("{}.{:02}", cents / 100, cents % 100)
}

fn validate_template_items(template: &Value, items: &[Value]) -> Result<()> {
    let available = template["data"]["templateList"]
        .as_array()
        .ok_or_else(|| anyhow!("wbx template response did not include templateList"))?;
    for item in items {
        let (class_code, class_name) = item_parts(item)?;
        let found = available.iter().any(|candidate| {
            candidate["jjflkmbh"].as_str() == Some(class_code)
                && candidate["jjflkmmc"].as_str() == Some(class_name)
        });
        if !found {
            bail!("reimbursement item {class_code} is not available for the selected project")
        }
    }
    Ok(())
}

fn save_payload(a: &Value, submit: bool) -> Result<Value> {
    let ywbh = required(a, "ywbh")?;
    let ywlx = a
        .get("ywlx")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_YWLX);
    if ywlx != DEFAULT_YWLX {
        bail!("wsbx.draft.save currently accepts only ywlx {DEFAULT_YWLX}")
    }
    let project = a
        .get("project")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("project must be a JSON object"))?;
    let project_value = Value::Object(project.clone());
    let (bmbh, xmbh) = project_parts(&project_value)?;
    let bmbh = bmbh.to_owned();
    let xmbh = xmbh.to_owned();
    let phone = required(a, "phone")?;
    let items = a
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("items must be a JSON array"))?;
    let items = selected_items(items, submit)?;
    let payments = payments(a)?;
    let is_online = bool_arg(a, "is_online", false);
    let no_paper = delivery_option(a, "no_paper", submit)?;
    let state = if !submit {
        "0"
    } else if is_online {
        "8"
    } else {
        "5"
    };
    Ok(json!({
        "ywbh": ywbh,
        "saveType": if submit { "1" } else { "0" },
        "bxsxList": items,
        "ybbxBaseInfo": {
            "bxsy": a.get("purpose").and_then(Value::as_str).unwrap_or(""),
            "bxqksm": a.get("description").and_then(Value::as_str).unwrap_or("")
        },
        "lxdh": phone,
        "zt": state,
        "isOnline": if is_online { "1" } else { "0" },
        "noPaper": no_paper,
        "cjkList": payments["cjkList"],
        "gwkList": payments["gwkList"],
        "xmzzList": payments["xmzzList"],
        "dszzList": payments["dszzList"],
        "dgzzList": payments["dgzzList"],
        "bxsxType": "zdhb",
        "project": {"bmbh": bmbh, "xmbh": xmbh}
    }))
}

impl<'a> WsbxClient<'a> {
    fn new(session: &'a Session) -> Result<Self> {
        let auth = match session.read_private_sidecar(SIDECAR)? {
            Some(contents) => match serde_json::from_str::<StoredAuth>(&contents) {
                Ok(auth)
                    if !auth.token.is_empty()
                        && !auth.user_id.is_empty()
                        && !auth.user_name.is_empty() =>
                {
                    Some(auth)
                }
                _ => {
                    session.remove_private_sidecar(SIDECAR)?;
                    None
                }
            },
            None => None,
        };
        Ok(Self { session, auth })
    }

    fn save_auth(&self) -> Result<()> {
        let auth = self
            .auth
            .as_ref()
            .ok_or_else(|| anyhow!("wbx authentication is unavailable"))?;
        let encoded = serde_json::to_string(auth).context("Could not serialize wsbx session")?;
        self.session.write_private_sidecar(SIDECAR, &encoded)
    }

    async fn ensure_authenticated(&mut self) -> Result<()> {
        if self.auth.is_none() {
            self.login().await?;
        }
        Ok(())
    }

    async fn login(&mut self) -> Result<()> {
        let csrf = self
            .session
            .cookie_value("info.tsinghua.edu.cn", "XSRF-TOKEN")?
            .ok_or_else(|| {
                anyhow!("information-portal CSRF cookie missing; authentication required")
            })?;
        let mut redirect = Url::parse(INFO_REDIRECT)?;
        redirect
            .query_pairs_mut()
            .append_pair("yyfwid", WSBX_APP_ID)
            .append_pair("machine", "p")
            .append_pair("_csrf", &csrf);
        let response = self
            .session
            .client
            .post(redirect)
            .header("X-XSRF-TOKEN", &csrf)
            .header("Origin", "https://info.tsinghua.edu.cn")
            .header(
                "Referer",
                "https://info.tsinghua.edu.cn/f/info/gxfw_fg/common/index",
            )
            .send()
            .await
            .map_err(|_| anyhow!("information-portal SSO request failed"))?;
        if matches!(response.status().as_u16(), 401 | 403) {
            bail!("information-portal CSRF cookie missing; authentication expired")
        }
        let response = response
            .error_for_status()
            .map_err(|_| anyhow!("information-portal SSO request was rejected"))?;
        let value: Value = response
            .json()
            .await
            .map_err(|_| anyhow!("information-portal session expired or returned a login page"))?;
        if value["result"] != "success" {
            bail!("information-portal SSO response was not successful")
        }
        let roaming_url = value["object"]["roamingurl"]
            .as_str()
            .ok_or_else(|| anyhow!("information-portal response omitted the roaming URL"))?
            .replace("&amp;", "&");
        let roaming = Url::parse(&roaming_url)?;
        if roaming.host_str() != Some("wsbx.tsinghua.edu.cn")
            || roaming.path() != "/wsyy/tsinghuaSso"
        {
            bail!("information-portal returned an unexpected reimbursement destination")
        }
        let ticket = roaming
            .query_pairs()
            .find(|(key, _)| key == "ticket")
            .map(|(_, value)| value.into_owned())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("reimbursement SSO ticket missing"))?;

        let raw = self
            .raw_api(
                Method::POST,
                "/login/sso",
                json!({"ticket": ticket}),
                Some("common"),
                None,
            )
            .await?;
        let token = response_token(&raw.headers)
            .ok_or_else(|| anyhow!("reimbursement login returned no rotating API token"))?;
        let envelope = validate_envelope(decrypt_response(&raw.body, &raw.aes_key)?)?;
        let user_id = required(&envelope["data"], "userId")?;
        let user_name = required(&envelope["data"], "userName")?;
        self.auth = Some(StoredAuth {
            token,
            user_id,
            user_name,
        });
        self.save_auth()
    }

    async fn raw_api(
        &self,
        method: Method,
        path: &str,
        data: Value,
        header: Option<&str>,
        token: Option<&str>,
    ) -> Result<RawApiResponse> {
        let (encrypted, aes_key) = encrypted_fields(&data)?;
        let url = api_url(path, header)?;
        let request = self.session.client.request(method.clone(), url);
        let mut request = if method == Method::GET {
            request.query(&encrypted)
        } else {
            request.json(&encrypted)
        };
        if let Some(token) = token {
            request = request.header("token", token);
        }
        let response = request
            .send()
            .await
            .map_err(|_| anyhow!("wbx API request failed"))?;
        let status = response.status();
        let headers = response.headers().clone();
        if matches!(status.as_u16(), 401 | 403) {
            return Err(auth_error());
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_API_RESPONSE)
        {
            bail!("wbx API response exceeds the safety limit")
        }
        let body = response
            .bytes()
            .await
            .context("Could not read wsbx API response")?;
        if body.len() as u64 > MAX_API_RESPONSE {
            bail!("wbx API response exceeds the safety limit")
        }
        if !status.is_success() {
            let detail = decrypt_response(&body, &aes_key).ok().and_then(|value| {
                let code = envelope_code(&value)?;
                let message = value
                    .get("msg")
                    .and_then(Value::as_str)
                    .unwrap_or("request rejected");
                Some(format!("{code}: {message}"))
            });
            match detail {
                Some(detail) => bail!("wbx API returned HTTP {} ({detail})", status.as_u16()),
                None => bail!("wbx API returned HTTP {}", status.as_u16()),
            }
        }
        Ok(RawApiResponse {
            headers,
            body: body.to_vec(),
            aes_key,
        })
    }

    async fn api(
        &mut self,
        method: Method,
        path: &str,
        data: Value,
        header: Option<&str>,
        allow_relogin: bool,
    ) -> Result<Value> {
        self.ensure_authenticated().await?;
        let mut attempted_relogin = false;
        loop {
            let token = self.auth.as_ref().ok_or_else(auth_error)?.token.clone();
            let result = self
                .raw_api(method.clone(), path, data.clone(), header, Some(&token))
                .await;
            let raw = match result {
                Ok(raw) => raw,
                Err(error)
                    if allow_relogin
                        && !attempted_relogin
                        && crate::auth::is_auth_failure(&error) =>
                {
                    attempted_relogin = true;
                    self.session.remove_private_sidecar(SIDECAR)?;
                    self.auth = None;
                    self.login().await?;
                    continue;
                }
                Err(error) => return Err(error),
            };
            if let Some(token) = response_token(&raw.headers) {
                self.auth.as_mut().ok_or_else(auth_error)?.token = token;
                self.save_auth()
                    .context("Could not persist the rotated wsbx API token")?;
            }
            let value = decrypt_response(&raw.body, &raw.aes_key)?;
            match validate_envelope(value) {
                Ok(value) => return Ok(value),
                Err(error)
                    if allow_relogin
                        && !attempted_relogin
                        && crate::auth::is_auth_failure(&error) =>
                {
                    attempted_relogin = true;
                    self.session.remove_private_sidecar(SIDECAR)?;
                    self.auth = None;
                    self.login().await?;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn post(&mut self, path: &str, data: Value, header: Option<&str>) -> Result<Value> {
        self.api(Method::POST, path, data, header, false).await
    }

    async fn post_read(&mut self, path: &str, data: Value, header: Option<&str>) -> Result<Value> {
        self.api(Method::POST, path, data, header, true).await
    }

    async fn get(&mut self, path: &str, data: Value, header: Option<&str>) -> Result<Value> {
        self.api(Method::GET, path, data, header, true).await
    }

    async fn get_mutation(
        &mut self,
        path: &str,
        data: Value,
        header: Option<&str>,
    ) -> Result<Value> {
        self.api(Method::GET, path, data, header, false).await
    }

    async fn upload(
        &mut self,
        path: &str,
        header: Option<&str>,
        file: &Path,
        fields: Value,
    ) -> Result<Value> {
        self.ensure_authenticated().await?;
        let metadata = fs::metadata(file).context("Could not inspect upload file")?;
        if !metadata.is_file() {
            bail!("Upload path must be a regular file")
        }
        if metadata.len() > MAX_UPLOAD_BYTES {
            bail!("Upload file exceeds the 30 MiB safety limit")
        }
        let filename = file
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| anyhow!("Upload filename must be valid UTF-8"))?
            .to_owned();
        let bytes = fs::read(file).context("Could not read upload file")?;
        let part = Part::bytes(bytes)
            .file_name(filename)
            .mime_str("application/octet-stream")
            .context("Invalid upload content type")?;
        let (encrypted, aes_key) = encrypted_fields(&fields)?;
        let data_header = encrypted["data"]
            .as_str()
            .ok_or_else(|| anyhow!("Encrypted upload fields are invalid"))?;
        let sec_header = encrypted["encSecKey"]
            .as_str()
            .ok_or_else(|| anyhow!("Encrypted upload key is invalid"))?;
        let token = self.auth.as_ref().ok_or_else(auth_error)?.token.clone();
        let response = self
            .session
            .client
            .post(api_url(path, header)?)
            .header("token", token)
            .header("Enc-Data", data_header)
            .header("Enc-Sec-Key", sec_header)
            .multipart(Form::new().part("file", part))
            .timeout(Duration::from_secs(300))
            .send()
            .await
            .map_err(|_| anyhow!("wbx upload request failed"))?;
        if matches!(response.status().as_u16(), 401 | 403) {
            bail!("wbx authentication expired during upload; the request outcome may be uncertain")
        }
        if !response.status().is_success() {
            bail!("wbx upload returned HTTP {}", response.status().as_u16())
        }
        let headers = response.headers().clone();
        let body = response
            .bytes()
            .await
            .context("Could not read wsbx upload response")?;
        if body.len() as u64 > MAX_API_RESPONSE {
            bail!("wbx upload response exceeds the safety limit")
        }
        if let Some(new_token) = response_token(&headers) {
            self.auth.as_mut().ok_or_else(auth_error)?.token = new_token;
            self.save_auth().context(
                "Could not persist the rotated wsbx token after upload; inspect the draft before retrying",
            )?;
        }
        let value = decrypt_response(&body, &aes_key)?;
        validate_envelope(value)
    }
}

fn response_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("token")
        .and_then(|value: &HeaderValue| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

async fn project_template(
    client: &mut WsbxClient<'_>,
    ywbh: &str,
    ywlx: &str,
    project: &Value,
) -> Result<Value> {
    let (bmbh, xmbh) = project_parts(project)?;
    client
        .post_read(
            "/zdhb/template/get",
            json!({
                "bmbh": bmbh,
                "xmbh": xmbh,
                "ywlx": ywlx,
                "ywbh": ywbh
            }),
            None,
        )
        .await
}

fn user_id(client: &WsbxClient<'_>) -> Result<String> {
    client
        .auth
        .as_ref()
        .map(|auth| auth.user_id.clone())
        .ok_or_else(auth_error)
}

pub fn operations() -> Vec<Value> {
    vec![
        json!({"name":"wsbx.user","mutating":false,"description":"Verify reimbursement SSO and return the current user profile","required":[],"optional":[],"upstream":"wsbx login/sso + userInfo"}),
        json!({"name":"wsbx.business-types","mutating":false,"description":"List enabled reimbursement business types","required":[],"optional":[],"upstream":"myReceipts/getYwlxs/get"}),
        json!({"name":"wsbx.project.list","mutating":false,"description":"Search projects available to the current user","required":[],"optional":["ywlx","query","department","principal","page_size","include_zero_balance"],"upstream":"common/funds/list"}),
        json!({"name":"wsbx.draft.list","mutating":false,"description":"List reimbursement drafts or submitted receipts","required":[],"optional":["ywlx","status","page_size","route","delegated","filter_delegation"],"upstream":"todoEs/getTodoLi"}),
        json!({"name":"wsbx.draft.get","mutating":false,"description":"Read a reimbursement draft by business number","required":["ywbh"],"optional":[],"upstream":"rmb/info/get"}),
        json!({"name":"wsbx.draft.status","mutating":false,"description":"Read the workflow state of a receipt","required":["ywbh"],"optional":[],"upstream":"common/get/ywbxr"}),
        json!({"name":"wsbx.draft.create","mutating":true,"description":"Allocate a new reimbursement business number without submitting a receipt","required":[],"optional":["ywlx"],"upstream":"common/ywbh/get"}),
        json!({"name":"wsbx.draft.template","mutating":false,"description":"Read available reimbursement expense items for a project","required":["ywbh","project"],"optional":["ywlx"],"upstream":"zdhb/template/get"}),
        json!({"name":"wsbx.draft.bind-project","mutating":true,"description":"Bind a project to a draft and return its reimbursement-item template","required":["ywbh","project"],"optional":["ywlx","old_project","operator"],"upstream":"common/updateXmInfo"}),
        json!({"name":"wsbx.invoice.list","mutating":false,"description":"List electronic invoices by pending or used reimbursement state","required":[],"optional":["ywbh","query","page_size","status"],"upstream":"invoice/list"}),
        json!({"name":"wsbx.invoice.bind","mutating":true,"description":"Bind selected electronic invoices to a reimbursement item","required":["ywbh","item","invoice_numbers"],"optional":["ywlx"],"upstream":"zdhb/dzfp/bind"}),
        json!({"name":"wsbx.invoice.unbind","mutating":true,"description":"Unbind one electronic invoice from a reimbursement item","required":["ywbh","invoice_number"],"optional":[],"upstream":"zdhb/dzfp/mx/delete"}),
        json!({"name":"wsbx.invoice.set-amount","mutating":true,"description":"Set the reimbursed amount of an electronic invoice","required":["ywbh","invoice_number","amount"],"optional":[],"upstream":"zdhb/dzfp/sbje/update"}),
        json!({"name":"wsbx.invoice.detail","mutating":false,"description":"Read electronic invoices attached to a reimbursement item","required":["ywbh","item"],"optional":[],"upstream":"zdhb/dzfp/mx/get"}),
        json!({"name":"wsbx.attachment.types","mutating":false,"description":"List required and optional attachment categories for a draft","required":["ywbh"],"optional":[],"upstream":"jh/fj/get"}),
        json!({"name":"wsbx.attachment.list","mutating":false,"description":"List attachments on a draft","required":["ywbh"],"optional":["ywlx"],"upstream":"file/get/list"}),
        json!({"name":"wsbx.attachment.upload","mutating":true,"description":"Upload one attachment to a draft","required":["ywbh","file","attachment_type"],"optional":["ywlx"],"upstream":"file/common/upload/file"}),
        json!({"name":"wsbx.attachment.delete","mutating":true,"description":"Delete one attachment from a draft","required":["attachment"],"optional":["fjly"],"upstream":"file/uploadfile/delete"}),
        json!({"name":"wsbx.draft.save","mutating":true,"description":"Validate and save reimbursement fields, items and payments as a draft","required":["ywbh","project","phone","items"],"optional":["ywlx","purpose","description","payments","no_paper"],"upstream":"pay/whole/submit"}),
        json!({"name":"wsbx.draft.submit","mutating":true,"description":"Submit a fully prepared daily reimbursement; inspect server state before retrying","required":["ywbh","project","phone","items"],"optional":["ywlx","purpose","description","payments","is_online","no_paper"],"upstream":"pay/whole/submit"}),
        json!({"name":"wsbx.draft.delete","mutating":true,"description":"Permanently delete a draft by business number","required":["ywbh"],"optional":[],"upstream":"todoEs/receipt/delete"}),
        json!({"name":"wsbx.draft.revoke","mutating":true,"description":"Request withdrawal of a submitted receipt by business number","required":["ywbh"],"optional":[],"upstream":"todoEs/receipt/revoke"}),
    ]
}

pub async fn execute(session: &Session, operation: &str, a: &Value) -> Result<Value> {
    let mut client = WsbxClient::new(session)?;
    client.ensure_authenticated().await?;
    match operation {
        "wsbx.user" => {
            let ygbh = user_id(&client)?;
            let profile = client
                .post_read("/userInfo/ygInfoByYgbh/get", json!({"ygbh": ygbh}), None)
                .await?;
            let employee = client
                .get("/common/employee", json!({"ygbh": user_id(&client)?}), None)
                .await?;
            Ok(json!({
                "user_id": user_id(&client)?,
                "user_name": client.auth.as_ref().map(|auth| auth.user_name.clone()),
                "profile": profile["data"],
                "employee": employee["data"]
            }))
        }
        "wsbx.business-types" => {
            client
                .post_read("/myReceipts/getYwlxs/get", json!({}), None)
                .await
        }
        "wsbx.project.list" => {
            let ywlx = a
                .get("ywlx")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_YWLX);
            let page_size = u64_arg(a, "page_size", 20)?;
            let mut data = json!({
                "pageSize": page_size,
                "pageNumber": 0,
                "current": 1,
                "ywlx": ywlx,
                "showZrye0": if bool_arg(a, "include_zero_balance", false) { "1" } else { "0" },
                "bxr": user_id(&client)?
            });
            if let Some(query) = a.get("query").and_then(Value::as_str) {
                data["searchXmbh"] = json!(query);
            }
            if let Some(department) = a.get("department").and_then(Value::as_str) {
                data["searchBmbh"] = json!(department);
            }
            if let Some(principal) = a.get("principal").and_then(Value::as_str) {
                data["searchFzr"] = json!(principal);
            }
            client.post_read("common/funds/list", data, None).await
        }
        "wsbx.draft.list" => {
            let ywlx = a
                .get("ywlx")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_YWLX);
            client
                .post_read(
                    "/todoEs/getTodoLi",
                    json!({
                        "pageSize": u64_arg(a, "page_size", 20)?,
                        "pageNumber": 0,
                        "current": 1,
                        "ywlx": ywlx,
                        "zt": a.get("status").and_then(Value::as_str).unwrap_or("ALL-2"),
                        "route": a.get("route").and_then(Value::as_str).unwrap_or("zdhb"),
                        "dcl": "1",
                        "sflz": a.get("filter_delegation").and_then(Value::as_str).unwrap_or("0"),
                        "delegated": a.get("delegated").and_then(Value::as_str).unwrap_or("")
                    }),
                    None,
                )
                .await
        }
        "wsbx.draft.get" => {
            client
                .get("/rmb/info/get", json!({"ywbh": required(a, "ywbh")?}), None)
                .await
        }
        "wsbx.draft.status" => {
            client
                .get(
                    "/common/get/ywbxr",
                    json!({"ywbh": required(a, "ywbh")?}),
                    None,
                )
                .await
        }
        "wsbx.draft.create" => {
            let ywlx = a
                .get("ywlx")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_YWLX);
            let result = client
                .get_mutation(
                    "/common/ywbh/get",
                    json!({"ywlx": ywlx, "jbrbh": user_id(&client)?}),
                    None,
                )
                .await?;
            let ywbh = result["data"]
                .as_str()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("wbx did not return a business number"))?
                .to_owned();
            Ok(json!({
                "business_number": ywbh,
                "ywlx": ywlx,
                "saved": false,
                "next": [
                    "wsbx.draft.bind-project",
                    "wsbx.invoice.list",
                    "wsbx.invoice.bind",
                    "wsbx.attachment.upload",
                    "wsbx.draft.save"
                ]
            }))
        }
        "wsbx.draft.template" => {
            let ywbh = required(a, "ywbh")?;
            let ywlx = a
                .get("ywlx")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_YWLX);
            let project = a
                .get("project")
                .ok_or_else(|| anyhow!("project must be a JSON object"))?;
            project_template(&mut client, &ywbh, ywlx, project).await
        }
        "wsbx.draft.bind-project" => {
            let ywbh = required(a, "ywbh")?;
            let ywlx = a
                .get("ywlx")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_YWLX);
            let project = a
                .get("project")
                .ok_or_else(|| anyhow!("project must be a JSON object"))?;
            let (bmbh, xmbh) = project_parts(project)?;
            let old_project = a.get("old_project").unwrap_or(&Value::Null);
            let old_bmbh = old_project
                .get("bmbh")
                .and_then(Value::as_str)
                .unwrap_or("");
            let old_xmbh = old_project
                .get("xmbh")
                .and_then(Value::as_str)
                .unwrap_or("");
            let operator = a
                .get("operator")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or(user_id(&client)?);
            let binding = client
                .post(
                    "/common/updateXmInfo",
                    json!({
                        "bmbh": bmbh,
                        "xmbh": xmbh,
                        "ywbh": ywbh,
                        "bmbhOld": old_bmbh,
                        "xmbhOld": old_xmbh,
                        "ywlx": ywlx,
                        "czr": operator
                    }),
                    None,
                )
                .await?;
            let (template, template_error) =
                match project_template(&mut client, &ywbh, ywlx, project).await {
                    Ok(template) => (template, Value::Null),
                    Err(error) => (Value::Null, json!(error.to_string())),
                };
            Ok(json!({
                "business_number": ywbh,
                "project_bound": true,
                "binding": binding,
                "template": template,
                "template_error": template_error
            }))
        }
        "wsbx.invoice.list" => {
            client
                .post_read(
                    "/invoice/list",
                    json!({
                        "pageSize": u64_arg(a, "page_size", 20)?,
                        "pageNumber": 0,
                        "current": 1,
                        "userId": user_id(&client)?,
                        "ywbh": a.get("ywbh").and_then(Value::as_str).unwrap_or(""),
                        "fpStatus": invoice_list_status(a)?,
                        "condition": a.get("query").and_then(Value::as_str).unwrap_or("")
                    }),
                    Some("invoice"),
                )
                .await
        }
        "wsbx.invoice.bind" => {
            let ywbh = required(a, "ywbh")?;
            let ywlx = a
                .get("ywlx")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_YWLX);
            let item = a
                .get("item")
                .ok_or_else(|| anyhow!("item must be a JSON object"))?;
            let (class_code, class_name) = item_parts(item)?;
            let invoice_numbers = a
                .get("invoice_numbers")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow!("invoice_numbers must be a JSON array"))?;
            if invoice_numbers.is_empty() {
                bail!("invoice_numbers must not be empty")
            }
            let invoice_numbers = invoice_numbers
                .iter()
                .map(|number| required(&json!({"number": number}), "number"))
                .collect::<Result<Vec<_>>>()?;
            let result = client
                .post(
                    "/zdhb/dzfp/bind",
                    json!({
                        "dzfphs": invoice_numbers,
                        "jjflkmbh": class_code,
                        "jjflkmmc": class_name,
                        "ywbh": ywbh,
                        "ywlx": ywlx,
                        "czr": user_id(&client)?,
                        "czrmc": client.auth.as_ref().map(|auth| auth.user_name.as_str()).unwrap_or("")
                    }),
                    None,
                )
                .await?;
            Ok(json!({"business_number": ywbh, "bound": true, "result": result["data"]}))
        }
        "wsbx.invoice.unbind" => {
            let ywbh = required(a, "ywbh")?;
            let invoice_number = required(a, "invoice_number")?;
            let result = client
                .get_mutation(
                    "/zdhb/dzfp/mx/delete",
                    json!({"ywbh": ywbh, "dzfph": invoice_number}),
                    None,
                )
                .await?;
            Ok(json!({"business_number": ywbh, "unbound": true, "result": result["data"]}))
        }
        "wsbx.invoice.set-amount" => {
            let ywbh = required(a, "ywbh")?;
            let invoice_number = required(a, "invoice_number")?;
            let amount = required(a, "amount")?;
            let cents = amount_cents(&json!(amount))?;
            let result = client
                .get_mutation(
                    "/zdhb/dzfp/sbje/update",
                    json!({
                        "ywbh": ywbh,
                        "dzfph": invoice_number,
                        "dzpjSbje": format_amount(cents)
                    }),
                    None,
                )
                .await?;
            Ok(json!({"business_number": ywbh, "amount_updated": true, "result": result["data"]}))
        }
        "wsbx.invoice.detail" => {
            let item = a
                .get("item")
                .ok_or_else(|| anyhow!("item must be a JSON object"))?;
            let (class_code, class_name) = item_parts(item)?;
            client
                .get(
                    "/zdhb/dzfp/mx/get",
                    json!({
                        "ywbh": required(a, "ywbh")?,
                        "jjflkmbh": class_code,
                        "jjflkmmc": class_name
                    }),
                    None,
                )
                .await
        }
        "wsbx.attachment.types" => {
            client
                .post_read(
                    "/jh/fj/get",
                    json!({"ywbh": required(a, "ywbh")?}),
                    Some("common"),
                )
                .await
        }
        "wsbx.attachment.list" => {
            client
                .post_read(
                    "/file/get/list",
                    json!({
                        "ywbh": required(a, "ywbh")?,
                        "mbbh": "QT",
                        "ywlx": a.get("ywlx").and_then(Value::as_str).unwrap_or(DEFAULT_YWLX),
                        "fjly": "ywPage"
                    }),
                    None,
                )
                .await
        }
        "wsbx.attachment.upload" => {
            let file = PathBuf::from(required(a, "file")?);
            let attachment_type = a
                .get("attachment_type")
                .ok_or_else(|| anyhow!("attachment_type must be a JSON object"))?;
            let fjlxdm = match required(attachment_type, "fjlxid") {
                Ok(value) => value,
                Err(_) => required(attachment_type, "fjlx")?,
            };
            let fjlxmc = match required(attachment_type, "fjlxmc") {
                Ok(value) => value,
                Err(_) => required(attachment_type, "fjmc")?,
            };
            let ywbh = required(a, "ywbh")?;
            let ywlx = a
                .get("ywlx")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_YWLX);
            let result = client
                .upload(
                    "/file/common/upload/file",
                    None,
                    &file,
                    json!({
                        "ywbh": ywbh,
                        "ywlx": ywlx,
                        "fjlxdm": fjlxdm,
                        "mbbh": "QT",
                        "fjType": fjlxdm,
                        "fjlxmc": fjlxmc,
                        "fjly": "ywPage"
                    }),
                )
                .await?;
            Ok(json!({"business_number": ywbh, "uploaded": true, "result": result["data"]}))
        }
        "wsbx.attachment.delete" => {
            let attachment = a
                .get("attachment")
                .and_then(Value::as_object)
                .ok_or_else(|| anyhow!("attachment must be a JSON object"))?;
            let mut data = Map::new();
            for key in ["uuid", "fjid", "filepath", "filename"] {
                data.insert(
                    key.to_owned(),
                    json!(required(&Value::Object(attachment.clone()), key)?),
                );
            }
            data.insert(
                "fjly".to_owned(),
                a.get("fjly")
                    .and_then(Value::as_str)
                    .map(|value| json!(value))
                    .unwrap_or_else(|| json!("ywPage")),
            );
            client
                .post("/file/uploadfile/delete", Value::Object(data), None)
                .await
        }
        "wsbx.draft.save" | "wsbx.draft.submit" => {
            let submit = operation == "wsbx.draft.submit";
            let mut payload = save_payload(a, submit)?;
            let ywbh = required(a, "ywbh")?;
            let ywlx = a
                .get("ywlx")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_YWLX);
            let project = a
                .get("project")
                .ok_or_else(|| anyhow!("project must be a JSON object"))?;
            if submit {
                let existing = client
                    .get("/common/get/ywbxr", json!({"ywbh": ywbh}), None)
                    .await?;
                let state = existing["data"]["zt"]
                    .as_str()
                    .ok_or_else(|| anyhow!("receipt status is unavailable; refusing to submit"))?;
                if !matches!(state, "0" | "Y1" | "Y4") {
                    bail!("receipt state {state} is not submit-ready; inspect it before retrying")
                }
            }
            let template = project_template(&mut client, &ywbh, ywlx, project).await?;
            let selected = payload["bxsxList"]
                .as_array()
                .ok_or_else(|| anyhow!("Saved reimbursement items are invalid"))?;
            validate_template_items(&template, selected)?;
            payload
                .as_object_mut()
                .ok_or_else(|| anyhow!("Saved reimbursement payload is invalid"))?
                .remove("project");
            let result = client.post("/pay/whole/submit", payload, None).await?;
            let warnings = result["data"]["warnMessage"].clone();
            Ok(json!({
                "business_number": ywbh,
                "saved": !submit,
                "submitted": submit,
                "warnings": warnings,
                "result": result["data"]
            }))
        }
        "wsbx.draft.delete" => {
            let ywbh = required(a, "ywbh")?;
            let result = client
                .post("/todoEs/receipt/delete", json!({"ywbh": ywbh}), None)
                .await?;
            Ok(json!({"business_number": ywbh, "deleted": true, "result": result["data"]}))
        }
        "wsbx.draft.revoke" => {
            let ywbh = required(a, "ywbh")?;
            let result = client
                .post("/todoEs/receipt/revoke", json!({"ywbh": ywbh}), None)
                .await?;
            Ok(json!({"business_number": ywbh, "revoke_requested": true, "result": result["data"]}))
        }
        _ => bail!("Unknown wsbx operation: {operation}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[test]
    fn aes_roundtrip_matches_ecb_pkcs7_contract() {
        let key = "0123456789abcdef";
        let plaintext = br#"{"data":{"hello":"world"}}"#;
        let encrypted = aes_encrypt(plaintext, key).unwrap();
        assert_eq!(
            aes_decrypt(&encrypted, key).unwrap().as_str(),
            std::str::from_utf8(plaintext).unwrap()
        );
    }

    #[test]
    fn endpoint_contract_is_bounded() {
        assert_eq!(
            api_url("/common/ywbh/get", None).unwrap().as_str(),
            "https://wsbx.tsinghua.edu.cn/wsyy/api/v1/wsyy-api/api/wsyy-service/common/ywbh/get"
        );
        assert!(api_url("../evil", None).is_err());
        assert!(api_url("/ok", Some("bad/header")).is_err());
    }

    #[test]
    fn invoice_list_status_maps_pending_and_used_states() {
        assert_eq!(invoice_list_status(&json!({})).unwrap(), "1");
        assert_eq!(
            invoice_list_status(&json!({"status":"pending"})).unwrap(),
            "1"
        );
        assert_eq!(invoice_list_status(&json!({"status":"used"})).unwrap(), "0");
        assert_eq!(
            invoice_list_status(&json!({"status":"completed"})).unwrap(),
            "0"
        );
        assert!(invoice_list_status(&json!({"status":"invalid"})).is_err());
    }

    #[test]
    fn selected_items_use_exact_decimal_cents() {
        let items = selected_items(
            &[json!({
                "jjflkmbh": "302",
                "jjflkmmc": "差旅费",
                "dzpjSbje": "12.34",
                "zzpjBxje": "0.66",
                "dzpjFjzs": "1",
                "zzpjFjzs": "2"
            })],
            true,
        )
        .unwrap();
        assert_eq!(items[0]["sbje"], "13.00");
        assert_eq!(items[0]["pzzshj"], 3);
        assert!(selected_items(
            &[json!({
                "jjflkmbh": "302",
                "jjflkmmc": "差旅费",
                "dzpjSbje": "0.001"
            })],
            true,
        )
        .is_err());
    }

    #[test]
    fn save_payload_has_frontend_contract() {
        let payload = save_payload(
            &json!({
                "ywbh": "YB0000000000001",
                "project": {"bmbh": "999", "xmbh": "FIXTURE"},
                "phone": "10000000000",
                "purpose": "测试",
                "items": [{
                    "jjflkmbh": "302",
                    "jjflkmmc": "差旅费",
                    "dzpjSbje": "10.00",
                    "dzpjFjzs": "1"
                }]
            }),
            false,
        )
        .unwrap();
        assert_eq!(payload["saveType"], "0");
        assert_eq!(payload["zt"], "0");
        assert_eq!(payload["noPaper"], "");
        assert_eq!(payload["bxsxType"], "zdhb");
        assert_eq!(payload["dszzList"], json!([]));
    }

    #[test]
    fn mutation_catalogue_guards_receipt_changes() {
        let operations = operations();
        for name in [
            "wsbx.draft.create",
            "wsbx.draft.bind-project",
            "wsbx.invoice.bind",
            "wsbx.attachment.upload",
            "wsbx.draft.save",
            "wsbx.draft.submit",
            "wsbx.draft.delete",
            "wsbx.draft.revoke",
        ] {
            assert!(
                operations
                    .iter()
                    .any(|operation| operation["name"] == name && operation["mutating"] == true),
                "{name}"
            );
        }
    }

    #[tokio::test]
    async fn encrypted_get_response_is_decoded() {
        let server = MockServer::start().await;
        let key = "0123456789abcdef";
        let ciphertext =
            aes_encrypt(br#"{"code":"1001","msg":"ok","data":{"value":1}}"#, key).unwrap();
        Mock::given(method("POST"))
            .and(path("/api/wsyy-service/example"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("token", "fixture-token")
                    .set_body_json(json!(ciphertext)),
            )
            .mount(&server)
            .await;
        let response = reqwest::Client::new()
            .post(format!("{}/api/wsyy-service/example", server.uri()))
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        let headers = response.headers().clone();
        let body = response.bytes().await.unwrap();
        assert_eq!(response_token(&headers).as_deref(), Some("fixture-token"));
        let value: Value = serde_json::from_slice(&body).unwrap();
        let encoded = value.as_str().unwrap();
        assert_eq!(
            decrypt_response(encoded.as_bytes(), key).unwrap()["data"]["value"],
            1
        );
    }
}
