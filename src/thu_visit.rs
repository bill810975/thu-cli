//! Tsinghua temporary-visitor reporting adapter.
//!
//! The `预约入校` family spans two systems: `visit.tsinghua.edu.cn` for
//! temporary visitor reporting and THOS for student/relative visitor cards.
//! This module handles the former end to end and exposes the exact cross-system
//! workflow identifiers so THOS forms remain discoverable through `thos.*`.

use anyhow::{anyhow, bail, Context, Result};
use reqwest::{Client, Method};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use url::Url;

use crate::{
    auth::{self, LoginOptions},
    credential,
    session::Session,
};

const ROOT: &str = "https://visit.tsinghua.edu.cn";
const FRONTEND: &str = "https://visit.tsinghua.edu.cn/visitor/pc/index.html";
const API_ROOT: &str = "https://visit.tsinghua.edu.cn/visitor/mbpm";
const SSO_RETURN_PATH: &str = "/visitor/pc/sso";
const SSO_CLIENT_ID: &str = "04655f71b042b9d2d8d10cd566bd9c39";
const CURRENT_APP: &str = "campusAppointment";
const TOKEN_SIDECAR: &str = "visit-auth";
const THOS_ROOT: &str = "https://thos.tsinghua.edu.cn";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct StoredAuth {
    #[serde(default)]
    token: String,
    #[serde(default)]
    user: Value,
}

struct VisitApi<'a> {
    client: Client,
    token: String,
    user: Value,
    _session: std::marker::PhantomData<&'a Session>,
}

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
            "visit.auth-status",
            false,
            "Current temporary-visitor-reporting account and private session state",
            &[],
            &[],
            "visit.tsinghua.edu.cn/visitor/mbpm/identity/isAdmin"
        ),
        operation(
            "visit.workflows",
            false,
            "Map the Info cards in the 预约入校 family to their native systems",
            &[],
            &[],
            "Info onlineAppRedirect + current visitor frontends"
        ),
        operation(
            "visit.thos-enter",
            false,
            "Use the Info application ticket to establish a read-only THOS visitor session",
            &["workflow"],
            &[],
            "information-portal SSO + thos.tsinghua.edu.cn"
        ),
        operation(
            "visit.thos-status",
            false,
            "Check the existing THOS visitor session without requesting a new SSO ticket",
            &["workflow"],
            &[],
            "thos.tsinghua.edu.cn/fp/fp/formHome/allNum"
        ),
        operation(
            "visit.thos-service-status",
            false,
            "Check whether one visitor service is open and within its monthly and concurrent-application limits",
            &["workflow"],
            &[],
            "thos.tsinghua.edu.cn/fp/fp/serveapply/{checkService,checkLimittime,checkServiceSyncLimit}"
        ),
        operation(
            "visit.thos-apply-info",
            false,
            "Read the visitor service metadata and server-generated form/process identifiers without submitting",
            &["workflow"],
            &[],
            "thos.tsinghua.edu.cn/fp/fp/serveapply/{serveInfo,getServeApply}"
        ),
        operation(
            "visit.thos-form-load",
            false,
            "Open the current visitor form through THOS formParser and return a content summary without submitting",
            &["workflow"],
            &["seq_id", "seq_pid"],
            "thos.tsinghua.edu.cn/fp/formParser"
        ),
        operation(
            "visit.thos-read",
            false,
            "Call one allowlisted read-only THOS visitor endpoint after SSO",
            &["workflow", "method", "path"],
            &["query", "payload"],
            "thos.tsinghua.edu.cn/fp read-only API catalogue"
        ),
        operation(
            "visit.menus",
            false,
            "List temporary-visitor menus and form routes for the current account",
            &[],
            &[],
            "visit.tsinghua.edu.cn/visitor/mbpm/function/getAllMenus"
        ),
        operation(
            "visit.portal",
            false,
            "Read the current visitor portal and its available portlets",
            &[],
            &[],
            "visit.tsinghua.edu.cn/visitor/mbpm/portal/getPortlets"
        ),
        operation(
            "visit.unread-count",
            false,
            "Read unread visitor-system message and notice counters",
            &[],
            &["notices"],
            "visit.tsinghua.edu.cn/visitor/mbpm/message"
        ),
        operation(
            "visit.task-records",
            false,
            "Read approval/task history for one visitor form instance",
            &["form_did", "form_tid"],
            &["process_instance"],
            "visit.tsinghua.edu.cn/visitor/mbpm/form-collaborate/getTaskRecordInfo"
        ),
        operation(
            "visit.form-preload",
            false,
            "Preload one visitor form without submitting it",
            &["form_did", "form_tid"],
            &["query", "process_instance"],
            "visit.tsinghua.edu.cn/visitor/mbpm/form/preLoad"
        ),
        operation(
            "visit.read",
            false,
            "Call one allowlisted read-only temporary-visitor endpoint",
            &["method", "path"],
            &["query", "payload"],
            "visit.tsinghua.edu.cn/visitor/mbpm allowlisted read catalogue"
        ),
        operation(
            "visit.form-submit",
            true,
            "Submit one fully reviewed visitor-reporting form payload",
            &["payload"],
            &["form_did", "form_tid"],
            "visit.tsinghua.edu.cn/visitor/mbpm/form/submit"
        )
    ])
}

pub async fn execute(session: &Session, operation: &str, args: &Value) -> Result<Value> {
    if operation == "visit.workflows" {
        return Ok(workflow_map());
    }
    if operation.starts_with("visit.thos-") {
        return thos_operation(session, operation, args).await;
    }
    let api = VisitApi::open(session).await?;
    match operation {
        "visit.auth-status" => {
            let value = api
                .raw(Method::POST, "/identity/isAdmin", &[], None, true)
                .await?;
            response_checked(&value)?;
            Ok(json!({
                "authenticated": true,
                "account": api.user.summary(),
                "admin": value["data"],
                "session_storage": "private mode-600 sidecar"
            }))
        }
        "visit.menus" => {
            let value = api
                .raw(Method::POST, "/function/getAllMenus", &[], None, true)
                .await?;
            response_checked(&value)?;
            Ok(value)
        }
        "visit.portal" => {
            let value = api
                .raw(
                    Method::GET,
                    "/portal/getPortlets",
                    &[("portalUuid", String::new())],
                    None,
                    true,
                )
                .await?;
            response_checked(&value)?;
            Ok(value)
        }
        "visit.unread-count" => {
            let message_path = if args["notices"].as_bool().unwrap_or(false) {
                "/message/getUnReadedNotices"
            } else {
                "/message/getUnReadedMsgCount"
            };
            let value = api.raw(Method::GET, message_path, &[], None, true).await?;
            response_checked(&value)?;
            Ok(value)
        }
        "visit.task-records" => {
            let mut params = vec![
                ("matrix_form_did", required_string(args, "form_did")?),
                ("matrix_form_tid", required_string(args, "form_tid")?),
            ];
            if let Some(id) = optional_string(args, "process_instance") {
                params.push(("piid", id));
            }
            let value = api
                .raw(
                    Method::GET,
                    "/form-collaborate/getTaskRecordInfo",
                    &params,
                    None,
                    true,
                )
                .await?;
            response_checked(&value)?;
            Ok(value)
        }
        "visit.form-preload" => form_preload(&api, args).await,
        "visit.read" => read_operation(&api, args).await,
        "visit.form-submit" => {
            let mut payload = required_object(args, "payload")?.clone();
            let object = payload
                .as_object_mut()
                .ok_or_else(|| anyhow!("payload must be a JSON object"))?;
            if let Some(form_did) = optional_string(args, "form_did") {
                object
                    .entry("matrix_form_did")
                    .or_insert_with(|| json!(form_did));
            }
            if let Some(form_tid) = optional_string(args, "form_tid") {
                object
                    .entry("matrix_form_tid")
                    .or_insert_with(|| json!(form_tid));
            }
            let value = api
                .raw(Method::POST, "/form/submit", &[], Some(&payload), true)
                .await?;
            response_checked(&value)?;
            Ok(value)
        }
        _ => bail!("unknown temporary-visitor operation: {operation}"),
    }
}

impl<'a> VisitApi<'a> {
    async fn open(session: &'a Session) -> Result<Self> {
        let stored = load_auth(session)?;
        if !stored.token.is_empty() {
            let api = Self {
                client: session.client.clone(),
                token: stored.token,
                user: stored.user,
                _session: std::marker::PhantomData,
            };
            let probe = api
                .raw(Method::POST, "/function/getAllMenus", &[], None, true)
                .await;
            if probe
                .as_ref()
                .is_ok_and(|value| response_checked(value).is_ok())
            {
                return Ok(api);
            }
        }
        visit_login(session).await
    }

    async fn raw(
        &self,
        method: Method,
        path: &str,
        params: &[(&str, String)],
        payload: Option<&Value>,
        authenticated: bool,
    ) -> Result<Value> {
        validate_api_path(path)?;
        let mut url = Url::parse(&format!("{API_ROOT}{path}"))?;
        if !params.is_empty() {
            let mut query = url.query_pairs_mut();
            for (key, value) in params {
                query.append_pair(key, value);
            }
        }
        let mut request = self
            .client
            .request(method, url)
            .header("Accept", "application/json, text/plain, */*")
            .header("X-Requested-With", "XMLHttpRequest")
            .header("mlangge", "zh-CN")
            .header("matrixcurrentappid", CURRENT_APP)
            .header("Origin", ROOT)
            .header("Referer", FRONTEND);
        if authenticated {
            if self.token.is_empty() {
                bail!("temporary-visitor private token is unavailable")
            }
            request = request.header("token", &self.token);
        }
        if let Some(payload) = payload {
            let object = payload
                .as_object()
                .ok_or_else(|| anyhow!("payload must be a JSON object"))?;
            let mut form = reqwest::multipart::Form::new();
            for (key, value) in object {
                if key == "X-Requested-With" {
                    continue;
                }
                let value = match value {
                    Value::String(value) => value.clone(),
                    Value::Bool(_) | Value::Number(_) => value.to_string(),
                    Value::Null => String::new(),
                    Value::Array(_) | Value::Object(_) => serde_json::to_string(value)?,
                };
                form = form.text(key.clone(), value);
            }
            form = form.text("X-Requested-With", "XMLHttpRequest");
            request = request.multipart(form);
        }
        let response = request
            .send()
            .await
            .context("temporary-visitor request failed")?;
        let status = response.status();
        let text = response
            .text()
            .await
            .context("reading temporary-visitor response")?;
        if !status.is_success() {
            if matches!(status.as_u16(), 401 | 403) {
                bail!("temporary-visitor session expired")
            }
            bail!("temporary-visitor returned HTTP {status}")
        }
        serde_json::from_str(&text).context("temporary-visitor returned malformed JSON")
    }
}

async fn visit_login(session: &Session) -> Result<VisitApi<'_>> {
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
    let code = visit_direct_code(session, &options).await?;

    let unsigned = VisitApi {
        client: session.client.clone(),
        token: String::new(),
        user: Value::Null,
        _session: std::marker::PhantomData,
    };
    unsigned
        .raw(Method::POST, "/identity/isAdmin", &[], None, false)
        .await
        .context("initializing temporary-visitor HTTP session failed")?;
    let exchange = unsigned
        .raw(
            Method::POST,
            "/identity/ssoLogin",
            &[],
            Some(&json!({"clientId": SSO_CLIENT_ID, "code": code})),
            false,
        )
        .await?;
    let exchange = sso_checked(&exchange)?;
    let token = exchange["data"]["token"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("temporary-visitor login returned no access token"))?
        .to_owned();
    let user = exchange["data"]["user"].clone();
    if user["account"].as_str() != Some(username.as_str())
        && user["userNo"].as_str() != Some(username.as_str())
    {
        bail!("temporary-visitor session belongs to a different identity");
    }
    let stored = StoredAuth {
        token: token.clone(),
        user: user.clone(),
    };
    session.write_private_sidecar(
        TOKEN_SIDECAR,
        &serde_json::to_string(&stored).context("serializing temporary-visitor session")?,
    )?;
    Ok(VisitApi {
        client: session.client.clone(),
        token,
        user,
        _session: std::marker::PhantomData,
    })
}

async fn visit_direct_code(session: &Session, options: &LoginOptions) -> Result<String> {
    let entry_url = format!(
        "https://id.tsinghua.edu.cn/do/off/ui/auth/login/form/{SSO_CLIENT_ID}/0?{SSO_RETURN_PATH}"
    );
    let sso_client =
        session.client_with_redirect_policy(reqwest::redirect::Policy::custom(|attempt| {
            let target = attempt.url();
            if target.host_str() == Some("visit.tsinghua.edu.cn")
                && target.path() == SSO_RETURN_PATH
            {
                attempt.stop()
            } else if attempt.previous().len() >= 15 {
                attempt.error("too many temporary-visitor authentication redirects")
            } else {
                attempt.follow()
            }
        }))?;
    let response = sso_client
        .get(entry_url)
        .send()
        .await
        .context("starting temporary-visitor SSO")?;
    if let Some(code) = callback_code(response.url()) {
        return Ok(code);
    }
    if let Some(callback) = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| response.url().join(value).ok())
    {
        if let Some(code) = callback_code(&callback) {
            return Ok(code);
        }
    }
    let response = response
        .error_for_status()
        .context("temporary-visitor SSO returned an HTTP error")?;
    let entry = response
        .text()
        .await
        .context("reading temporary-visitor SSO response")?;
    let callback = auth::destination_login(&session.auth_client, &entry, options).await?;
    let callback = Url::parse(&callback).context("invalid temporary-visitor SSO callback")?;
    callback_code(&callback)
        .ok_or_else(|| anyhow!("temporary-visitor callback did not include an authorization code"))
}

fn callback_code(url: &Url) -> Option<String> {
    if validate_callback(url).is_err() {
        return None;
    }
    url.query_pairs()
        .find(|(key, _)| key == "code" || key == "ticket")
        .map(|(_, value)| value.into_owned())
        .filter(|value| !value.is_empty())
}

async fn thos_operation(session: &Session, operation: &str, args: &Value) -> Result<Value> {
    let workflow = required_string(args, "workflow")?;
    let app = workflow_app(&workflow)?;
    if operation == "visit.thos-status" {
        return Ok(json!({
            "authenticated": true,
            "workflow": workflow,
            "system": "thos.tsinghua.edu.cn",
            "counts": thos_counts(session).await?
        }));
    }
    let counts = thos_session_counts(session, &app).await?;
    match operation {
        "visit.thos-enter" => Ok(json!({
            "authenticated": true,
            "workflow": workflow,
            "name": app.name,
            "system": "thos.tsinghua.edu.cn",
            "counts": counts
        })),
        "visit.thos-service-status" => thos_service_status(session, &workflow, &app).await,
        "visit.thos-apply-info" => thos_apply_info(session, &workflow, &app).await,
        "visit.thos-form-load" => thos_form_load(session, &workflow, &app, args).await,
        "visit.thos-read" => thos_read(session, args).await,
        _ => bail!("unknown THOS visitor operation: {operation}"),
    }
}

async fn thos_session_counts(session: &Session, app: &WorkflowApp) -> Result<Value> {
    let counts = match thos_counts(session).await {
        Ok(counts) => counts,
        Err(_) => {
            let roaming = info_app_redirect(session, app.yyfwid).await?;
            if roaming.host_str() != Some("thos.tsinghua.edu.cn") {
                bail!("information-portal returned an unexpected THOS destination")
            }
            let response = session
                .client
                .get(roaming)
                .send()
                .await
                .context("opening the THOS visitor service failed")?
                .error_for_status()
                .context("THOS visitor SSO was rejected")?;
            if response
                .url()
                .host_str()
                .is_none_or(|host| host != "thos.tsinghua.edu.cn")
            {
                bail!("THOS visitor SSO did not remain on the expected campus host")
            }
            thos_counts(session)
                .await
                .context("THOS visitor SSO did not establish a usable session")?
        }
    };
    Ok(counts)
}

async fn thos_counts(session: &Session) -> Result<Value> {
    let counts = thos_json_post(session, "/fp/fp/formHome/allNum", &json!({})).await?;
    if counts["zbNum"].is_null() || counts["AuditSvsNum"].is_null() {
        bail!("THOS visitor session is not authenticated")
    }
    Ok(counts)
}

async fn thos_service_status(
    session: &Session,
    workflow: &str,
    app: &WorkflowApp,
) -> Result<Value> {
    let service_check = thos_post_text(
        session,
        "/fp/fp/serveapply/checkService",
        &json!({"serveID": app.service_id}),
    )
    .await
    .map(|value| (value.trim() == "0", value.trim().to_owned()));
    let monthly_check = thos_post_text(
        session,
        "/fp/fp/serveapply/checkLimittime",
        &json!({"serviceID": app.service_id}),
    )
    .await
    .map(|value| (value.trim() == "OK", value.trim().to_owned()));
    let concurrent_check = thos_post_text(
        session,
        "/fp/fp/serveapply/checkServiceSyncLimit",
        &json!({"serviceID": app.service_id}),
    )
    .await
    .map(|value| (value.trim() == "OK", value.trim().to_owned()));
    Ok(json!({
        "authenticated": true,
        "workflow": workflow,
        "name": app.name,
        "service_id": app.service_id,
        "open_and_within_limit": text_check_result(service_check),
        "monthly_limit": text_check_result(monthly_check),
        "concurrent_application_limit": text_check_result(concurrent_check)
    }))
}

fn text_check_result(result: Result<(bool, String)>) -> Value {
    match result {
        Ok((ok, value)) => json!({"ok": ok, "provider_result": value}),
        Err(error) => json!({"ok": false, "error": error.to_string()}),
    }
}

async fn thos_apply_info(session: &Session, workflow: &str, app: &WorkflowApp) -> Result<Value> {
    let service = thos_json_post(
        session,
        "/fp/fp/serveapply/serveInfo",
        &json!({"serviceID": app.service_id}),
    )
    .await?;
    let apply = thos_json_post(
        session,
        "/fp/fp/serveapply/getServeApply",
        &json!({"serveID": app.service_id, "from": "hall"}),
    )
    .await?;
    let service_data = service.get("data").unwrap_or(&service);
    let form_id = json_string(&apply, "formID")
        .or_else(|| json_string(service_data, "FORMID"))
        .unwrap_or_else(|| app.form_id.to_owned());
    let process_id = json_string(&apply, "procID")
        .or_else(|| json_string(service_data, "WORKFLOWTEMPID"))
        .unwrap_or_else(|| app.workflow_temp_id.to_owned());
    let form_url = thos_form_url(app.service_id, &form_id, &process_id, "", "");
    Ok(json!({
        "authenticated": true,
        "workflow": workflow,
        "name": app.name,
        "service_id": app.service_id,
        "service": {
            "name": service_data["NAME"].as_str().unwrap_or(app.name),
            "form_id": service_data["FORMID"].as_str().unwrap_or(app.form_id),
            "workflow_temp_id": service_data["WORKFLOWTEMPID"].as_str().unwrap_or(app.workflow_temp_id),
            "type": service_data["URL"],
            "version": service_data["VERSION"]
        },
        "form": {
            "form_id": form_id,
            "process_id": process_id,
            "name": apply["name"],
            "app_type": apply["app_type"],
            "record": apply["record"],
            "temp_save_authorized": apply["tempSaveAuth"],
            "ai_form": apply["AIForm"],
            "form_url": form_url
        }
    }))
}

async fn thos_form_load(
    session: &Session,
    workflow: &str,
    app: &WorkflowApp,
    args: &Value,
) -> Result<Value> {
    let info = thos_apply_info(session, workflow, app).await?;
    let form_id = info["form"]["form_id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .unwrap_or(app.form_id);
    let process_id = info["form"]["process_id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .unwrap_or(app.workflow_temp_id);
    let form_url = thos_form_url(
        app.service_id,
        form_id,
        process_id,
        args["seq_id"].as_str().unwrap_or_default(),
        args["seq_pid"].as_str().unwrap_or_default(),
    );
    let response = session
        .client
        .get(&form_url)
        .header("Accept", "text/html,application/xhtml+xml")
        .header("Referer", format!("{THOS_ROOT}/fp/view?m=fp"))
        .send()
        .await
        .context("loading THOS visitor form failed")?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let html = response
        .text()
        .await
        .context("reading THOS visitor form failed")?;
    if !status.is_success() {
        bail!("THOS formParser returned HTTP {status}")
    }
    let document = Html::parse_document(&html);
    let title = Selector::parse("title")
        .ok()
        .and_then(|selector| document.select(&selector).next())
        .map(|element| element.text().collect::<String>().trim().to_owned())
        .filter(|value| !value.is_empty());
    let form_count = Selector::parse("form")
        .ok()
        .map(|selector| document.select(&selector).count())
        .unwrap_or_default();
    let input_count = Selector::parse("input,textarea,select")
        .ok()
        .map(|selector| document.select(&selector).count())
        .unwrap_or_default();
    let field_names = Selector::parse("input[name],textarea[name],select[name]")
        .ok()
        .map(|selector| {
            document
                .select(&selector)
                .filter_map(|element| element.value().attr("name"))
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .take(200)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let variable_names = Selector::parse("[variablebind]")
        .ok()
        .map(|selector| {
            document
                .select(&selector)
                .filter_map(|element| element.value().attr("variablebind"))
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .take(200)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(json!({
        "authenticated": true,
        "workflow": workflow,
        "name": app.name,
        "status": status.as_u16(),
        "content_type": content_type,
        "content_length": html.len(),
        "title": title,
        "form_count": form_count,
        "input_count": input_count,
        "field_names": field_names,
        "variable_names": variable_names,
        "contains_web_form": html.contains("WebForm"),
        "form_url": form_url,
        "submitted": false
    }))
}

fn thos_form_url(
    service_id: &str,
    form_id: &str,
    process_id: &str,
    seq_id: &str,
    seq_pid: &str,
) -> String {
    let mut url = Url::parse(&format!("{THOS_ROOT}/fp/formParser"))
        .expect("the THOS formParser URL is a constant");
    url.query_pairs_mut()
        .append_pair("status", "select")
        .append_pair("formid", form_id)
        .append_pair("service_id", service_id)
        .append_pair("process", process_id)
        .append_pair("seqId", seq_id)
        .append_pair("seqPid", seq_pid);
    url.to_string()
}

fn json_string(value: &Value, key: &str) -> Option<String> {
    value[key]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

async fn thos_json_post(session: &Session, path: &str, payload: &Value) -> Result<Value> {
    let text = thos_post_text(session, path, payload).await?;
    serde_json::from_str(&text).context("THOS returned malformed JSON")
}

async fn thos_post_text(session: &Session, path: &str, payload: &Value) -> Result<String> {
    if !path.starts_with("/fp/fp/") {
        bail!("unsafe THOS API path")
    }
    let response = session
        .client
        .post(format!("{THOS_ROOT}{path}"))
        .header("Accept", "application/json, text/plain, */*")
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Origin", THOS_ROOT)
        .header("Referer", format!("{THOS_ROOT}/fp/view?m=fp"))
        .json(payload)
        .send()
        .await
        .context("THOS visitor request failed")?;
    let status = response.status();
    let text = response
        .text()
        .await
        .context("reading THOS visitor response failed")?;
    if !status.is_success() {
        bail!("THOS returned HTTP {status}")
    }
    Ok(text)
}

async fn thos_read(session: &Session, args: &Value) -> Result<Value> {
    let method = required_string(args, "method")?.to_ascii_uppercase();
    let path = required_string(args, "path")?;
    if !thos_read_allowed(&method, &path) {
        bail!("This THOS endpoint/method is not on the read-only allowlist");
    }
    let mut url = Url::parse("https://thos.tsinghua.edu.cn")?;
    url.set_path(path.trim_start_matches('/'));
    if path.starts_with('/') {
        url.set_path(&path);
    }
    for (key, value) in query_params(args.get("query"))? {
        url.query_pairs_mut().append_pair(&key, &value);
    }
    let method = Method::from_bytes(method.as_bytes()).context("invalid HTTP method")?;
    let mut request = session
        .client
        .request(method.clone(), url)
        .header("Accept", "application/json")
        .header("Origin", "https://thos.tsinghua.edu.cn")
        .header("Referer", "https://thos.tsinghua.edu.cn/fp/visitService");
    if method == Method::POST {
        request = request.json(
            args.get("payload")
                .filter(|value| value.is_object())
                .unwrap_or(&json!({})),
        );
    }
    let response = request.send().await.context("THOS read request failed")?;
    let status = response.status();
    let text = response
        .text()
        .await
        .context("reading THOS response failed")?;
    if !status.is_success() {
        bail!("THOS returned HTTP {status}")
    }
    serde_json::from_str(&text).context("THOS returned malformed JSON")
}

fn thos_read_allowed(method: &str, path: &str) -> bool {
    if !matches!(method, "GET" | "POST") || !path.starts_with("/fp/fp/") {
        return false;
    }
    [
        "/fp/fp/formHome/",
        "/fp/fp/myserviceapply/",
        "/fp/fp/taskcenter/",
        "/fp/fp/draft/",
        "/fp/fp/carboncopy/",
        "/fp/fp/aggregation/",
        "/fp/fp/serveapply/applyhistory",
        "/fp/fp/serveapply/serveinstance",
        "/fp/fp/Uniformcommon/select",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
}

struct WorkflowApp {
    name: &'static str,
    yyfwid: &'static str,
    service_id: &'static str,
    form_id: &'static str,
    workflow_temp_id: &'static str,
}

fn workflow_app(workflow: &str) -> Result<WorkflowApp> {
    match workflow {
        "student_visitor" => Ok(WorkflowApp {
            name: "学生访客（无身份证人员）预约",
            yyfwid: "3E5289463C389D0101E9BE262CDA49DD",
            service_id: "611741148012544",
            form_id: "b97336a8-8c2c-4b55-8f21-48b960db",
            workflow_temp_id: "af48e03d-3ced-4bf2-9bbb-b8939e8af3e3",
        }),
        "relative_visitor" => Ok(WorkflowApp {
            name: "亲友来访人员报备",
            yyfwid: "29D037F2FD63BA8209FA410A8FBDD799",
            service_id: "27277790547968",
            form_id: "a33a1092-b8bd-49a3-be61-38b41ed8",
            workflow_temp_id: "1eb41f5a-80d0-4876-b7bd-98f334802d3d",
        }),
        _ => bail!("workflow must be student_visitor or relative_visitor"),
    }
}

async fn info_app_redirect(session: &Session, yyfwid: &str) -> Result<Url> {
    const REDIRECT_URL: &str =
        "https://info.tsinghua.edu.cn/b/yyfw/vyyfwxx/info/portal_fg/common/onlineAppRedirect";
    let csrf = session
        .cookie_value("info.tsinghua.edu.cn", "XSRF-TOKEN")?
        .filter(|value| !value.trim().is_empty())
        .context("information-portal CSRF cookie missing; authentication required")?;
    let mut url = Url::parse(REDIRECT_URL)?;
    url.query_pairs_mut()
        .append_pair("yyfwid", yyfwid)
        .append_pair("machine", "p")
        .append_pair("_csrf", &csrf);
    let response = session
        .client
        .post(url)
        .header("X-XSRF-TOKEN", &csrf)
        .header("Origin", "https://info.tsinghua.edu.cn")
        .header(
            "Referer",
            "https://info.tsinghua.edu.cn/f/info/portal_fg/student/yyfwxxindex",
        )
        .send()
        .await
        .context("information-portal SSO redirect failed")?
        .error_for_status()
        .context("information-portal SSO redirect was rejected")?;
    let value: Value = response
        .json()
        .await
        .context("information-portal SSO redirect returned malformed JSON")?;
    if value["result"] != "success" {
        bail!("information-portal did not resolve the requested application")
    }
    let destination = value["object"]["roamingurl"]
        .as_str()
        .ok_or_else(|| anyhow!("information-portal SSO redirect omitted the destination"))?
        .replace("&amp;", "&");
    let destination = Url::parse(&destination)?;
    if destination.scheme() != "https" {
        bail!("information-portal returned a non-HTTPS application destination")
    }
    Ok(destination)
}

async fn form_preload(api: &VisitApi<'_>, args: &Value) -> Result<Value> {
    let mut params = vec![
        (
            "matrix_form_did".to_owned(),
            required_string(args, "form_did")?,
        ),
        (
            "matrix_form_tid".to_owned(),
            required_string(args, "form_tid")?,
        ),
    ];
    params.extend(query_params(args.get("query"))?);
    if let Some(id) = optional_string(args, "process_instance") {
        params.push(("piid".into(), id));
    }
    let borrowed = params
        .iter()
        .map(|(key, value)| (key.as_str(), value.clone()))
        .collect::<Vec<_>>();
    let value = api
        .raw(Method::GET, "/form/preLoad", &borrowed, None, true)
        .await?;
    response_checked(&value)?;
    Ok(value)
}

async fn read_operation(api: &VisitApi<'_>, args: &Value) -> Result<Value> {
    let method = required_string(args, "method")?.to_ascii_uppercase();
    let path = required_string(args, "path")?;
    if !read_allowed(&method, &path) {
        bail!("This temporary-visitor endpoint/method is not on the read-only allowlist");
    }
    let params = query_params(args.get("query"))?;
    let borrowed = params
        .iter()
        .map(|(key, value)| (key.as_str(), value.clone()))
        .collect::<Vec<_>>();
    let payload = args.get("payload").filter(|value| value.is_object());
    let method = Method::from_bytes(method.as_bytes()).context("invalid HTTP method")?;
    let value = api.raw(method, &path, &borrowed, payload, true).await?;
    response_checked(&value)?;
    Ok(value)
}

fn read_allowed(method: &str, path: &str) -> bool {
    let path = path.trim_start_matches('/');
    let common = path == "identity/isAdmin"
        || path == "identity/signon"
        || path == "portal/getSecAppInfo"
        || path == "portal/getPortlets"
        || path == "function/getAllMenus"
        || path == "message/getUnReadedMsgCount"
        || path == "message/getUnReadedNotices"
        || path.starts_with("form/preLoad")
        || path.starts_with("form/load")
        || path.starts_with("form/loadData")
        || path.starts_with("form-queryList/get")
        || path.starts_with("form-queryList/load")
        || path == "form-collaborate/getTaskRecordInfo"
        || path == "form-collaborate/getTaskRecordInfo"
        || path.starts_with("org/load")
        || path.starts_with("org/get");
    if common {
        return method == "GET" || method == "POST";
    }
    false
}

fn query_params(value: Option<&Value>) -> Result<Vec<(String, String)>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("query must be a JSON object"))?;
    object
        .iter()
        .map(|(key, value)| {
            let value = match value {
                Value::String(value) => value.clone(),
                Value::Bool(_) | Value::Number(_) => value.to_string(),
                Value::Null => String::new(),
                _ => bail!("query.{key} must be a scalar"),
            };
            Ok((key.clone(), value))
        })
        .collect()
}

fn response_checked(value: &Value) -> Result<&Value> {
    if value["code"].is_null()
        || matches!(value["code"].as_i64(), Some(0 | 200))
        || value["success"].as_bool() == Some(true)
    {
        return Ok(value);
    }
    if matches!(value["code"].as_i64(), Some(401 | 403)) {
        bail!("temporary-visitor session expired")
    }
    bail!(
        "temporary-visitor rejected the request: {}",
        value["message"]
            .as_str()
            .unwrap_or("provider returned an unsuccessful response")
    )
}

fn sso_checked(value: &Value) -> Result<&Value> {
    if value["code"].is_null()
        || matches!(value["code"].as_i64(), Some(0 | 200))
        || value["success"].as_bool() == Some(true)
    {
        return Ok(value);
    }
    bail!(
        "temporary-visitor SSO exchange was rejected: {}",
        value["message"]
            .as_str()
            .or_else(|| value["msg"].as_str())
            .unwrap_or("provider returned an unsuccessful response")
    )
}

fn load_auth(session: &Session) -> Result<StoredAuth> {
    let Some(raw) = session.read_private_sidecar(TOKEN_SIDECAR)? else {
        return Ok(StoredAuth::default());
    };
    serde_json::from_str(&raw).context("temporary-visitor private session metadata is malformed")
}

fn validate_callback(url: &Url) -> Result<()> {
    if url.scheme() != "https"
        || url.host_str() != Some("visit.tsinghua.edu.cn")
        || url.path() != SSO_RETURN_PATH
    {
        bail!("temporary-visitor SSO returned an unexpected callback")
    }
    Ok(())
}

fn validate_api_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.starts_with("http://")
        || path.starts_with("https://")
        || path.contains('?')
        || path.contains('#')
        || path.split('/').any(|part| part == "..")
    {
        bail!("unsafe or malformed temporary-visitor API path")
    }
    Ok(())
}

fn required_string(args: &Value, key: &str) -> Result<String> {
    args[key]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .with_context(|| format!("missing string argument: {key}"))
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

fn workflow_map() -> Value {
    json!({
        "family": "预约入校",
        "account_note": "The current account may see only an entitled subset.",
        "workflows": [
            {
                "key": "temporary_visitor",
                "name": "临时来访报备",
                "yyfwid": "C52889552AA7F89258B3A38DF9DBAF27",
                "system": "visit.tsinghua.edu.cn",
                "entry_path": "/visitor/pc/sso",
                "native_operations": ["visit.*"]
            },
            {
                "key": "student_visitor",
                "name": "学生访客（无身份证人员）预约",
                "yyfwid": "3E5289463C389D0101E9BE262CDA49DD",
                "system": "thos.tsinghua.edu.cn",
                "entry_path": "/fp/visitService",
                "service_id": "611741148012544",
                "form_id": "b97336a8-8c2c-4b55-8f21-48b960db",
                "workflow_temp_id": "af48e03d-3ced-4bf2-9bbb-b8939e8af3e3",
                "native_operations": [
                    "visit.thos-service-status",
                    "visit.thos-apply-info",
                    "visit.thos-form-load",
                    "visit.thos-read"
                ]
            },
            {
                "key": "relative_visitor",
                "name": "亲友来访人员报备",
                "yyfwid": "29D037F2FD63BA8209FA410A8FBDD799",
                "system": "thos.tsinghua.edu.cn",
                "entry_path": "/fp/visitService",
                "service_id": "27277790547968",
                "form_id": "a33a1092-b8bd-49a3-be61-38b41ed8",
                "workflow_temp_id": "1eb41f5a-80d0-4876-b7bd-98f334802d3d",
                "native_operations": [
                    "visit.thos-service-status",
                    "visit.thos-apply-info",
                    "visit.thos-form-load",
                    "visit.thos-read"
                ]
            }
        ]
    })
}

trait UserSummary {
    fn summary(&self) -> Value;
}

impl UserSummary for Value {
    fn summary(&self) -> Value {
        json!({
            "account": self["account"],
            "user_no": self["userNo"],
            "name": self["userName"],
            "department": self["deptName"],
            "roles": self["roles"]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_allowlist_refuses_submission() {
        assert!(read_allowed("GET", "portal/getSecAppInfo"));
        assert!(read_allowed("GET", "portal/getPortlets"));
        assert!(read_allowed("POST", "form-queryList/getSearchInfo"));
        assert!(!read_allowed("POST", "form/submit"));
        assert!(!read_allowed("POST", "identity/logout"));
    }

    #[test]
    fn callback_is_school_only() {
        assert!(validate_callback(
            &Url::parse("https://visit.tsinghua.edu.cn/visitor/pc/sso?code=x").unwrap()
        )
        .is_ok());
        assert!(validate_callback(
            &Url::parse("https://example.com/visitor/pc/sso?code=x").unwrap()
        )
        .is_err());
    }

    #[test]
    fn workflows_cover_all_three_live_cards() {
        let map = workflow_map();
        assert_eq!(map["workflows"].as_array().unwrap().len(), 3);
        assert_eq!(map["workflows"][0]["key"], "temporary_visitor");
        assert_eq!(map["workflows"][1]["system"], "thos.tsinghua.edu.cn");
        assert_eq!(map["workflows"][1]["service_id"], "611741148012544");
    }

    #[test]
    fn form_parser_url_contains_exact_visitor_identifiers() {
        let app = workflow_app("student_visitor").unwrap();
        let url = thos_form_url(
            app.service_id,
            app.form_id,
            app.workflow_temp_id,
            "sequence",
            "parent",
        );
        let url = Url::parse(&url).unwrap();
        assert_eq!(url.path(), "/fp/formParser");
        let query = url
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(query["status"], "select");
        assert_eq!(query["formid"], app.form_id);
        assert_eq!(query["service_id"], app.service_id);
        assert_eq!(query["process"], app.workflow_temp_id);
    }

    #[test]
    fn school_host_constants_are_exact() {
        assert_eq!(
            Url::parse(ROOT).unwrap().host_str(),
            Some("visit.tsinghua.edu.cn")
        );
        assert_eq!(SSO_CLIENT_ID.len(), 32);
    }
}
