//! Current Tsinghua sports venue adapter.
//!
//! The previous `50.tsinghua.edu.cn/gymbook` service has been retired. This
//! adapter targets the unified venue platform at `sports.tsinghua.edu.cn`
//! and keeps the short-lived platform token in a private session sidecar.

use anyhow::{anyhow, bail, Context, Result};
use chrono::{FixedOffset, Utc};
use md5::{Digest, Md5};
use rand::Rng;
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use url::Url;

use crate::{
    auth::{self, LoginOptions},
    credential,
    session::Session,
};

const SPORTS_SITE: &str = "https://www.sports.tsinghua.edu.cn/venue/site";
const SPORTS_LOGIN: &str = "https://www.sports.tsinghua.edu.cn/venue/site/authcenter/toLoginPage";
const SPORTS_RETURN: &str = "https://www.sports.tsinghua.edu.cn/venue/";
const SIGN_APP_ID: &str = "1497016617475903488";
const SIGN_KEY: &str = "57325972627c40bd8c77296d39293705";
const SIGN_CHARS: &[u8] = b"ABCDEFGHJKMNPQRSTWXYZabcdefhijkmnprstwxyz0123456789";
const TOKEN_SIDECAR: &str = "sports-auth";

#[derive(Deserialize, Serialize)]
struct StoredToken {
    token: String,
    #[serde(default, rename = "refreshToken")]
    refresh_token: String,
}

struct SportsApi {
    client: Client,
    token: String,
}

pub fn operations() -> Value {
    json!([
        {
            "name": "sports.auth-status",
            "mutating": false,
            "description": "Current sports venue account and session state",
            "required": [],
            "optional": [],
            "upstream": "sports.tsinghua.edu.cn/venue"
        },
        {
            "name": "sports.scenes",
            "mutating": false,
            "description": "List current sports venues and bookable scenes",
            "required": [],
            "optional": ["query"],
            "upstream": "sports.tsinghua.edu.cn/venue"
        },
        {
            "name": "sports.areas",
            "mutating": false,
            "description": "List device kinds, buildings and rooms used to scope sports availability",
            "required": ["scene_uuid"],
            "optional": [],
            "upstream": "sports.tsinghua.edu.cn/venue"
        },
        {
            "name": "sports.site-types",
            "mutating": false,
            "description": "List bookable site types for a sports scene",
            "required": ["scene_uuid"],
            "optional": [],
            "upstream": "sports.tsinghua.edu.cn/venue"
        },
        {
            "name": "sports.scene-detail",
            "mutating": false,
            "description": "Read rules, opening hours and booking mode for a sports scene",
            "required": ["scene_uuid"],
            "optional": [],
            "upstream": "sports.tsinghua.edu.cn/venue"
        },
        {
            "name": "sports.availability",
            "mutating": false,
            "description": "List concise bookable courts, pitches, tables and time sessions",
            "required": ["scene_uuid", "date"],
            "optional": [
                "page_num",
                "page_size",
                "site_kind_id",
                "dev_kind_uuid",
                "class_type_uuid",
                "class_type_enum",
                "include_unavailable"
            ],
            "upstream": "sports.tsinghua.edu.cn/venue"
        },
        {
            "name": "sports.pay-types",
            "mutating": false,
            "description": "List online payment channels and accepted booking payment-mode values",
            "required": ["site_uuid", "site_type"],
            "optional": [],
            "upstream": "sports.tsinghua.edu.cn/venue"
        },
        {
            "name": "sports.records",
            "mutating": false,
            "description": "List sports reservations and generated orders",
            "required": [],
            "optional": ["page_num", "page_size"],
            "upstream": "sports.tsinghua.edu.cn/venue"
        },
        {
            "name": "sports.preflight",
            "mutating": false,
            "description": "Validate one exact session, payment mode and booking form without reserving it",
            "required": ["scene_uuid", "start", "end"],
            "optional": [
                "site_uuid",
                "site_name",
                "pay_type",
                "site_kind_id",
                "dev_kind_uuid",
                "class_type_uuid",
                "class_type_enum"
            ],
            "upstream": "sports.tsinghua.edu.cn/venue"
        },
        {
            "name": "sports.book",
            "mutating": true,
            "description": "Lock and book one available sports session; does not complete payment",
            "required": ["scene_uuid", "start", "end", "pay_type"],
            "optional": [
                "site_uuid",
                "site_name",
                "members",
                "captcha",
                "form_variables",
                "choose_candidates",
                "site_kind_id",
                "dev_kind_uuid",
                "class_type_uuid",
                "class_type_enum"
            ],
            "upstream": "sports.tsinghua.edu.cn/venue"
        },
        {
            "name": "sports.cancel",
            "mutating": true,
            "description": "Cancel one sports reservation",
            "required": ["resv_uuid"],
            "optional": [],
            "upstream": "sports.tsinghua.edu.cn/venue"
        }
    ])
}

pub async fn execute(session: &Session, operation: &str, args: &Value) -> Result<Value> {
    if operation == "sports.auth-status" {
        let api = SportsApi::open(session).await?;
        let user = api.get("/system/login/getLoginUser", &[]).await?;
        return Ok(json!({
            "authenticated": true,
            "account": user["data"]["account"],
            "name": user["data"]["name"],
            "login_type": user["data"]["loginType"],
            "session_storage": "private mode-600 sidecar"
        }));
    }

    let api = SportsApi::open(session).await?;
    match operation {
        "sports.scenes" | "sports.venues" => {
            let scenes = api.get("/api/site/scene/list", &[]).await?;
            let query = args["query"].as_str().unwrap_or_default().to_lowercase();
            let data = scenes["data"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|scene| {
                    query.is_empty()
                        || scene["sceneName"]
                            .as_str()
                            .is_some_and(|name| name.to_lowercase().contains(&query))
                })
                .map(normalize_scene)
                .collect::<Vec<_>>();
            Ok(json!({"count":data.len(),"scenes":data}))
        }
        "sports.areas" => load_areas(&api, &required(args, "scene_uuid")?).await,
        "sports.site-types" => {
            let value = api
                .get(
                    "/api/site/siteType",
                    &[("sceneUuid", required(args, "scene_uuid")?)],
                )
                .await?;
            Ok(value)
        }
        "sports.scene-detail" => {
            api.get(
                "/api/site/scene/detail",
                &[("uuid", required(args, "scene_uuid")?)],
            )
            .await
        }
        "sports.availability" => {
            let date = required(args, "date")?;
            validate_date(&date)?;
            let value = availability_page(
                &api,
                &required(args, "scene_uuid")?,
                &date,
                &AvailabilityOptions::from_args(args),
                optional_number(args, "page_size", 100),
                optional_number(args, "page_num", 1),
            )
            .await?;
            Ok(normalize_availability(
                &value,
                &date,
                &required(args, "scene_uuid")?,
                args["include_unavailable"].as_bool().unwrap_or(false),
            ))
        }
        "sports.pay-types" => {
            let value = api
                .get(
                    "/api/resv/trade/pay/type",
                    &[
                        ("terminal", "PC".to_owned()),
                        ("siteUuid", required(args, "site_uuid")?),
                        ("siteType", required(args, "site_type")?),
                    ],
                )
                .await?;
            Ok(json!({
                "online_payment_channels": value["data"].as_array()
                    .into_iter()
                    .flatten()
                    .map(|channel| json!({
                        "channel_id": channel["channelId"],
                        "name": channel["name"],
                        "property": channel["property"]
                    }))
                    .collect::<Vec<_>>(),
                "booking_payment_modes": ["PAY_ONLINE", "PAY_OFFLINE", "PAY_CARD"]
            }))
        }
        "sports.records" => {
            let body = json!({
                "pageSize": optional_number(args, "page_size", 30),
                "pageNum": optional_number(args, "page_num", 1),
                "resvCheckStatus": "",
                "resvStatus": "",
                "resvExtendStatus": "",
                "containsActivity": false
            });
            let value = api.post("/api/reserve/reserveRecord", &body, &[]).await?;
            Ok(normalize_records(&value))
        }
        "sports.preflight" => preflight(&api, args).await,
        "sports.book" => book(&api, args).await,
        "sports.cancel" => {
            let body = json!({"resvUuid": required(args, "resv_uuid")?});
            api.post("/api/reserve/cancelReserve", &body, &[]).await
        }
        _ => bail!("unknown sports operation: {operation}"),
    }
}

impl SportsApi {
    async fn open(session: &Session) -> Result<Self> {
        let profile = session.load_profile()?.ok_or_else(|| {
            anyhow!("Info login metadata is missing; log in once with --trust-device")
        })?;
        let username = profile.username;

        if let Some(stored) = load_token(session)? {
            let api = Self {
                client: session.client.clone(),
                token: stored.token,
            };
            if let Ok(user) = api.get_raw("/system/login/getLoginUser", &[]).await {
                if user["code"] == 0 && user["data"]["account"].as_str() == Some(username.as_str())
                {
                    return Ok(api);
                }
            }
        }

        let password = credential::get(&session.credential_path(), &username)?
            .map(|secret| secret.to_string())
            .unwrap_or_default();
        let login = LoginOptions {
            username: username.clone(),
            password,
            fingerprint: profile.fingerprint,
            service: "info".into(),
            two_factor_method: None,
            interactive: false,
            trust_device: true,
            use_webvpn: false,
        };
        let token = sports_login(session, &login).await?;
        let api = Self {
            client: session.client.clone(),
            token: token.token.clone(),
        };
        let user = api.get("/system/login/getLoginUser", &[]).await?;
        if user["data"]["account"].as_str() != Some(username.as_str()) {
            bail!("sports venue session belongs to a different identity");
        }
        session.write_private_sidecar(
            TOKEN_SIDECAR,
            &serde_json::to_string(&token).context("serializing sports session")?,
        )?;
        Ok(api)
    }

    async fn get(&self, path: &str, params: &[(&str, String)]) -> Result<Value> {
        let value = self.get_raw(path, params).await?;
        checked(value)
    }

    async fn get_raw(&self, path: &str, params: &[(&str, String)]) -> Result<Value> {
        self.request(Method::GET, path, params, None).await
    }

    async fn post(&self, path: &str, body: &Value, params: &[(&str, String)]) -> Result<Value> {
        let value = self.request(Method::POST, path, params, Some(body)).await?;
        checked(value)
    }

    async fn request(
        &self,
        method: Method,
        path: &str,
        params: &[(&str, String)],
        body: Option<&Value>,
    ) -> Result<Value> {
        request_signed(&self.client, Some(&self.token), method, path, params, body).await
    }
}

async fn sports_login(session: &Session, options: &LoginOptions) -> Result<StoredToken> {
    let mut start = Url::parse(SPORTS_LOGIN)?;
    start
        .query_pairs_mut()
        .append_pair("redirectUrl", SPORTS_RETURN)
        .append_pair("typeCode", "");
    let response = session
        .auth_client
        .get(start)
        .send()
        .await
        .context("starting sports venue SSO")?
        .error_for_status()
        .context("sports venue SSO returned an HTTP error")?;
    let entry = response
        .text()
        .await
        .context("reading sports venue SSO response")?;
    let callback = auth::destination_login(&session.auth_client, &entry, options).await?;
    let response = session
        .auth_client
        .get(callback)
        .send()
        .await
        .context("completing sports venue SSO")?
        .error_for_status()
        .context("sports venue callback returned an HTTP error")?;
    let callback_url = response.url().clone();
    let uni_token = callback_url
        .query_pairs()
        .find(|(key, _)| key == "uniToken")
        .map(|(_, value)| value.into_owned())
        .ok_or_else(|| anyhow!("sports venue callback did not provide a one-time SSO token"))?;
    let exchange = request_signed(
        &session.client,
        None,
        Method::POST,
        "/cas/token",
        &[],
        Some(&json!({
            "platForm": "CAS",
            "client": "PC",
            "token": uni_token
        })),
    )
    .await?;
    let exchange = checked(exchange)?;
    let token = exchange["data"]["token"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("sports venue token exchange returned no access token"))?
        .to_owned();
    Ok(StoredToken {
        token,
        refresh_token: exchange["data"]["refreshToken"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
    })
}

fn load_token(session: &Session) -> Result<Option<StoredToken>> {
    let Some(raw) = session.read_private_sidecar(TOKEN_SIDECAR)? else {
        return Ok(None);
    };
    Ok(serde_json::from_str(&raw).ok())
}

async fn request_signed(
    client: &Client,
    token: Option<&str>,
    method: Method,
    path: &str,
    params: &[(&str, String)],
    body: Option<&Value>,
) -> Result<Value> {
    let mut url = Url::parse(&format!("{SPORTS_SITE}{path}"))?;
    {
        let mut query = url.query_pairs_mut();
        for (key, value) in signature_params() {
            query.append_pair(key, &value);
        }
        for (key, value) in params {
            query.append_pair(key, value);
        }
    }
    let mut request = client
        .request(method, url)
        .header("Language-Set", "CN")
        .header("x-api-version", "2.0.0");
    if let Some(token) = token {
        request = request.header("token", token);
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request
        .send()
        .await
        .context("sports venue request failed")?;
    let status = response.status();
    let text = response
        .text()
        .await
        .context("reading sports venue response")?;
    if !status.is_success() {
        bail!("sports venue returned HTTP {status}");
    }
    serde_json::from_str(&text).context("sports venue returned malformed JSON")
}

fn checked(value: Value) -> Result<Value> {
    if value["code"].as_i64() != Some(0) {
        bail!(
            "sports venue rejected the request: {}",
            value["message"].as_str().unwrap_or("unknown error")
        );
    }
    Ok(value)
}

fn signature_params() -> [(&'static str, String); 4] {
    let timestamp = Utc::now().timestamp_millis();
    let nonce: String = {
        let mut rng = rand::thread_rng();
        (0..32)
            .map(|_| SIGN_CHARS[rng.gen_range(0..SIGN_CHARS.len())] as char)
            .collect()
    };
    let source = format!("appId={SIGN_APP_ID}&nonce={nonce}&timeStamp={timestamp}&key={SIGN_KEY}");
    let digest = Md5::digest(source.as_bytes());
    [
        ("appId", SIGN_APP_ID.to_owned()),
        ("timeStamp", timestamp.to_string()),
        ("nonce", nonce),
        ("sign", format!("{digest:x}")),
    ]
}

#[derive(Default)]
struct AvailabilityOptions {
    site_kind_id: Option<String>,
    dev_kind_uuid: Option<String>,
    class_type_uuid: Option<String>,
    class_type_enum: Option<String>,
}

impl AvailabilityOptions {
    fn from_args(args: &Value) -> Self {
        Self {
            site_kind_id: nonempty_string(&args["site_kind_id"]),
            dev_kind_uuid: nonempty_string(&args["dev_kind_uuid"]),
            class_type_uuid: nonempty_string(&args["class_type_uuid"]),
            class_type_enum: nonempty_string(&args["class_type_enum"]),
        }
    }
}

async fn load_areas(api: &SportsApi, scene_uuid: &str) -> Result<Value> {
    let device_kinds = api
        .get("/api/site/devKind/list", &[("uuid", scene_uuid.to_owned())])
        .await?["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|kind| {
            json!({
                "uuid": kind["uuid"],
                "name": kind["kindName"].as_str()
                    .or(kind["name"].as_str())
                    .or(kind["label"].as_str()),
                "value": kind["value"],
                "ext_info": kind["extInfo"],
                "class": kind["kindClass"]
            })
        })
        .collect::<Vec<_>>();
    let buildings = api
        .get(
            "/api/site/chooseByType",
            &[
                ("sceneUuid", scene_uuid.to_owned()),
                ("siteType", "BUILDING".into()),
            ],
        )
        .await?["data"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut building_results = Vec::new();
    for building in buildings {
        let building_uuid = building["uuid"].as_str().unwrap_or_default();
        let rooms = if building_uuid.is_empty() {
            Vec::new()
        } else {
            api.get(
                "/api/site/chooseByType",
                &[
                    ("siteUuid", building_uuid.to_owned()),
                    ("sceneUuid", scene_uuid.to_owned()),
                    ("siteType", "ROOM".into()),
                ],
            )
            .await?["data"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        };
        building_results.push(json!({
            "uuid": building["uuid"],
            "name": building["name"].as_str()
                .or(building["siteName"].as_str())
                .or(building["roomName"].as_str()),
            "rooms": rooms.into_iter().map(|room| json!({
                "uuid": room["uuid"],
                "name": room["name"].as_str()
                    .or(room["siteName"].as_str())
                    .or(room["roomName"].as_str())
            })).collect::<Vec<_>>()
        }));
    }
    Ok(json!({
        "scene_uuid": scene_uuid,
        "device_kinds": device_kinds,
        "buildings": building_results
    }))
}

async fn availability_page(
    api: &SportsApi,
    scene_uuid: &str,
    date: &str,
    options: &AvailabilityOptions,
    page_size: i64,
    page_num: i64,
) -> Result<Value> {
    let detail = api
        .get("/api/site/scene/detail", &[("uuid", scene_uuid.to_owned())])
        .await?["data"]
        .clone();
    let scene_use_type = match detail["sceneUseType"].as_i64().unwrap_or_default() {
        value if value & 1 != 0 => "NORMAL",
        value if value & 2 != 0 => "SPORT_GROUP",
        value if value & 4 != 0 => "SPORT_PERSON",
        _ => bail!("sports scene has no supported reservation mode"),
    };

    let buildings = api
        .get(
            "/api/site/chooseByType",
            &[
                ("sceneUuid", scene_uuid.to_owned()),
                ("siteType", "BUILDING".into()),
            ],
        )
        .await?["data"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let building_uuid = buildings
        .first()
        .and_then(|building| building["uuid"].as_str())
        .unwrap_or_default()
        .to_owned();
    let rooms = if building_uuid.is_empty() {
        Vec::new()
    } else {
        api.get(
            "/api/site/chooseByType",
            &[
                ("siteUuid", building_uuid.clone()),
                ("sceneUuid", scene_uuid.to_owned()),
                ("siteType", "ROOM".into()),
            ],
        )
        .await?["data"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    };
    let room_uuid = rooms
        .first()
        .and_then(|room| room["uuid"].as_str())
        .unwrap_or_default()
        .to_owned();
    let class_type_enum = options
        .class_type_enum
        .as_deref()
        .unwrap_or(if room_uuid.is_empty() {
            "BUILDING"
        } else {
            "ROOM"
        });
    let class_type_uuid = options
        .class_type_uuid
        .as_deref()
        .unwrap_or(if room_uuid.is_empty() {
            &building_uuid
        } else {
            &room_uuid
        });
    if class_type_enum != "BUILDING" && class_type_enum != "ROOM" {
        bail!("class_type_enum must be BUILDING or ROOM");
    }
    if class_type_uuid.is_empty() {
        bail!("the sports scene has no selectable building or room");
    }

    let dev_kind_uuid = if let Some(uuid) = options.dev_kind_uuid.as_deref() {
        uuid.to_owned()
    } else {
        api.get("/api/site/devKind/list", &[("uuid", scene_uuid.to_owned())])
            .await?["data"]
            .as_array()
            .and_then(|kinds| kinds.first())
            .and_then(|kind| kind["uuid"].as_str())
            .unwrap_or_default()
            .to_owned()
    };

    api.post(
        "/api/reserve/current/page",
        &json!({
            "sceneUuid": scene_uuid,
            "resvKind": "CURRENT_RESERVE",
            "devKindUuid": dev_kind_uuid,
            "siteType": detail["relatedType"],
            "searchValue": "",
            "siteKindId": options.site_kind_id.as_deref().unwrap_or_default(),
            "classTypeEnum": class_type_enum,
            "classTypeUuid": class_type_uuid,
            "reserveDate": date,
            "sceneUseType": scene_use_type,
            "pageSize": page_size,
            "pageNum": page_num
        }),
        &[],
    )
    .await
}

struct BookingTarget {
    site: Value,
    session: Value,
}

struct BookingForm {
    form_id: String,
    deploy_uuid: String,
    form_type: String,
    condition_variable_count: usize,
    audit_required: bool,
}

async fn booking_target(api: &SportsApi, args: &Value) -> Result<BookingTarget> {
    let scene_uuid = required(args, "scene_uuid")?;
    let start = required(args, "start")?;
    let end = required(args, "end")?;
    validate_window(&start, &end)?;
    let date = &start[..10];
    let page = availability_page(
        api,
        &scene_uuid,
        date,
        &AvailabilityOptions::from_args(args),
        999,
        1,
    )
    .await?;
    let sites = page["data"].as_array().cloned().unwrap_or_default();
    let site = find_site(&sites, args)?.clone();
    let session = find_session(&site, &start, &end)
        .ok_or_else(|| anyhow!("the requested time is not available; refresh sports.availability"))?
        .clone();
    Ok(BookingTarget { site, session })
}

async fn booking_form(api: &SportsApi, site: &Value) -> Result<BookingForm> {
    let form_id = site["formRuleVo"]["formUuid"]
        .as_str()
        .filter(|value| !value.is_empty())
        .or_else(|| site["formUuid"].as_str().filter(|value| !value.is_empty()))
        .unwrap_or("unifound")
        .to_owned();
    let brief = api
        .get(&format!("/workflow/process/brief/{form_id}"), &[])
        .await?;
    let deploy_uuid = brief["data"]["deployUuid"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("sports booking form has no workflow deployment"))?
        .to_owned();
    let form = api
        .get(&format!("/workflow/process/{deploy_uuid}"), &[])
        .await?["data"]
        .clone();
    Ok(BookingForm {
        form_id,
        deploy_uuid,
        form_type: form["type"].as_str().unwrap_or_default().to_owned(),
        condition_variable_count: form["conditionVariable"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default(),
        audit_required: form["auditRequired"].as_bool().unwrap_or(false),
    })
}

async fn preflight(api: &SportsApi, args: &Value) -> Result<Value> {
    let target = booking_target(api, args).await?;
    let form = booking_form(api, &target.site).await?;
    let captcha_config = api
        .get("/system/login/enableValidCode", &[])
        .await?
        .get("data")
        .cloned()
        .unwrap_or(Value::Null);
    let captcha_required = captcha_config["sysValue"].as_i64().unwrap_or_default() & 1 != 0;
    let normalized_session = normalize_session(&target.session);
    let pay_type = args["pay_type"]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            payment_mode(target.session["userFeeDetails"]["payType"].as_i64()).to_owned()
        });
    validate_payment_mode(&pay_type)?;
    Ok(json!({
        "ready_to_book": true,
        "confirmation_required": true,
        "site_uuid": target.site["uuid"],
        "site_name": target.site["siteName"],
        "site_type": target.site["siteType"],
        "location": target.site["siteLocation"],
        "start": normalized_session["start"],
        "end": normalized_session["end"],
        "session_uuid": normalized_session["session_uuid"],
        "price_cents": normalized_session["price_cents"],
        "price_yuan": normalized_session["price_yuan"],
        "pay_type": pay_type,
        "captcha_required": captcha_required,
        "form": {
            "form_id": form.form_id,
            "form_uuid": form.form_id,
            "deploy_uuid": form.deploy_uuid,
            "type": form.form_type,
            "audit_required": form.audit_required,
            "condition_variable_count": form.condition_variable_count,
            "custom_form_variables_required": form.form_type == "FLOW_FORM",
            "approval_candidates_required": form.form_type == "FLOW_FORM"
                && form.condition_variable_count == 0
        }
    }))
}

async fn book(api: &SportsApi, args: &Value) -> Result<Value> {
    let target = booking_target(api, args).await?;
    let site = &target.site;
    let session = &target.session;
    let scene_uuid = required(args, "scene_uuid")?;
    let start = required(args, "start")?;
    let end = required(args, "end")?;
    let pay_type = required(args, "pay_type")?;
    validate_payment_mode(&pay_type)?;

    let user = api.get("/system/login/getLoginUser", &[]).await?["data"].clone();
    let user_id = user["id"].as_str().unwrap_or_default().to_owned();
    if user_id.is_empty() {
        bail!("sports venue account has no user id");
    }
    let members = match args["members"].as_array() {
        Some(values) if !values.is_empty() => values.clone(),
        _ => vec![json!(user_id)],
    };

    let captcha_config = api
        .get("/system/login/enableValidCode", &[])
        .await?
        .get("data")
        .cloned()
        .unwrap_or(Value::Null);
    if captcha_config["sysValue"].as_i64().unwrap_or_default() & 1 != 0
        && args["captcha"].as_str().unwrap_or_default().is_empty()
    {
        bail!("this booking requires the site's human slider verification; complete it in the official venue page");
    }

    let form = booking_form(api, site).await?;
    if form.form_type == "FLOW_FORM" {
        if form.condition_variable_count > 0 && args["form_variables"].is_null() {
            bail!("this booking requires explicit form_variables");
        }
        if form.condition_variable_count == 0 && args["choose_candidates"].is_null() {
            bail!("this booking requires explicit approval choose_candidates");
        }
    }

    let reserve_time = json!([{
        "startTime": start,
        "endTime": end
    }]);
    let lock_config = api
        .post("/api/reserve/getLockConfig", &json!({}), &[])
        .await?;
    let lock_body = json!({
        "sceneUuid": scene_uuid,
        "resvKind": "CURRENT_RESERVE",
        "siteUuid": site["uuid"],
        "siteType": site["siteType"],
        "resvMember": members,
        "reserveTime": reserve_time,
        "formParam": {"formId": form.form_id},
        "lockTime": lock_config["data"]["reserveLockTime"]
    });
    let lock = api.post("/api/reserve/lockSite", &lock_body, &[]).await?;
    let lock_key = lock["data"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("sports venue did not return a reservation lock"))?
        .to_owned();

    let variables = args["form_variables"].clone();
    let choose_candidates = args["choose_candidates"].clone();
    let site_session = json!([{
        "sessionDetailUuid": session["uuid"],
        "reserveTime": {
            "startTime": start,
            "endTime": end
        }
    }]);
    let body = json!({
        "sceneUuid": scene_uuid,
        "sceneUseType": session["sceneUseType"],
        "siteUuid": site["uuid"],
        "siteType": site["siteType"],
        "reserveTime": reserve_time,
        "siteSessionReserve": site_session,
        "resvMember": members,
        "resvKind": "CURRENT_RESERVE",
        "payType": pay_type,
        "purchaseUuid": "",
        "lockKey": lock_key,
        "formParam": {
            "formId": form.form_id,
            "deployUuid": form.deploy_uuid,
            "variables": if variables.is_null() { json!({}) } else { variables },
            "chooseCandidates": if choose_candidates.is_null() { json!({}) } else { choose_candidates }
        },
        "captcha": args["captcha"].as_str().unwrap_or_default()
    });
    let result = match api.post("/api/reserve/addReserve", &body, &[]).await {
        Ok(value) => value,
        Err(error) => {
            let _ = api
                .post(
                    "/api/reserve/unLockSite",
                    &json!({"lockKey": lock_key}),
                    &[],
                )
                .await;
            return Err(error);
        }
    };
    let resv_ids = result["data"]["resvIds"]
        .as_array()
        .cloned()
        .or_else(|| result["data"].as_array().cloned())
        .unwrap_or_default();
    if resv_ids.is_empty() {
        bail!("sports venue returned no reservation ID after booking");
    }
    let order = api
        .post(
            "/resv/order/check",
            &json!({"resvUuidList":resv_ids,"userId":user_id}),
            &[],
        )
        .await?;
    Ok(json!({
        "booked": true,
        "payment_completed": false,
        "reservation": site["siteName"],
        "start": start,
        "end": end,
        "resv_ids": resv_ids,
        "order": order["data"],
        "next": "If orderGenerated is true, complete payment in the official venue/order page before the hold expires."
    }))
}

fn normalize_scene(scene: &Value) -> Value {
    json!({
        "scene_uuid": scene["uuid"],
        "scene_name": scene["sceneName"].as_str().or(scene["sceneEnName"].as_str()),
        "location": scene["location"],
        "description": scene["description"],
        "related_type": scene["relatedType"],
        "open_time": scene["openTime"],
        "telephone": scene["telPhone"],
        "status": scene["status"],
        "scene_show_type": scene["sceneShowType"],
        "scene_use_type": scene["sceneUseType"],
        "single_reserve_num": scene["singleReserveNum"]
    })
}

fn normalize_availability(
    value: &Value,
    date: &str,
    scene_uuid: &str,
    include_unavailable: bool,
) -> Value {
    let raw_sites = value["data"].as_array().cloned().unwrap_or_default();
    let mut unavailable_reasons = BTreeSet::new();
    let mut open_site_count = 0;
    let mut bookable_session_count = 0;
    let mut sites = Vec::new();

    for site in &raw_sites {
        let site_open = is_bookable(&site["reserveStatus"]);
        if site_open {
            open_site_count += 1;
        } else if let Some(reason) = reason(&site["reserveStatus"]) {
            unavailable_reasons.insert(reason);
        }

        let raw_sessions = site["sessionVo"].as_array().cloned().unwrap_or_default();
        let mut sessions = Vec::new();
        for session in raw_sessions {
            let session_open = is_bookable(&session["reserveStatus"]);
            if session_open {
                bookable_session_count += 1;
            } else if let Some(reason) = reason(&session["reserveStatus"]) {
                unavailable_reasons.insert(reason);
            }
            if include_unavailable || session_open {
                sessions.push(normalize_session(&session));
            }
        }
        if !include_unavailable && sessions.is_empty() {
            continue;
        }

        let form = &site["formRuleVo"];
        sites.push(json!({
            "site_uuid": site["uuid"],
            "site_name": site["siteName"],
            "site_type": site["siteType"],
            "site_kind": site["kindName"],
            "location": site["siteLocation"],
            "open": site_open,
            "reserve_status_reason": reason(&site["reserveStatus"]),
            "form_id": form["formUuid"].as_str()
                .or_else(|| site["formUuid"].as_str())
                .unwrap_or("unifound"),
            "form_uuid": form["formUuid"],
            "sessions": sessions
        }));
    }

    json!({
        "date": date,
        "scene_uuid": scene_uuid,
        "bookable_only": !include_unavailable,
        "count": sites.len(),
        "site_count": raw_sites.len(),
        "open_site_count": open_site_count,
        "bookable_session_count": bookable_session_count,
        "unavailable_reasons": unavailable_reasons,
        "sites": sites
    })
}

fn normalize_session(session: &Value) -> Value {
    let price_cents = session["userFeeDetails"]["chargingUnitPrice"]
        .as_i64()
        .or_else(|| {
            session["ruleFeeDetails"]
                .as_array()
                .and_then(|items| items.first())
                .and_then(|item| item["chargingUnitPrice"].as_i64())
        });
    let allowed = session["allowUserNum"].as_i64().unwrap_or_default();
    let reserved = session["resvUserNum"].as_i64().unwrap_or_default();
    json!({
        "session_uuid": session["uuid"],
        "start": normalized_date_time(session, "beginDate", "beginTime"),
        "end": normalized_date_time(session, "endDate", "endTime"),
        "scene_use_type": session["sceneUseType"],
        "reserve_status": session["reserveStatus"]["reserveStatus"],
        "reserve_status_reason": reason(&session["reserveStatus"]),
        "capacity": allowed,
        "reserved": reserved,
        "remaining": (allowed - reserved).max(0),
        "charging_mode": session["userFeeDetails"]["chargingMode"],
        "pay_type": session["userFeeDetails"]["payType"],
        "price_cents": price_cents,
        "price_yuan": price_cents.map(|cents| cents as f64 / 100.0)
    })
}

fn normalize_records(value: &Value) -> Value {
    let records = value["data"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|record| {
            let paid_amount_cents = record["paidAmount"].as_i64();
            json!({
                "reservation_uuid": record["resvUuid"],
                "scene_uuid": record["sceneUuid"],
                "scene_name": record["sceneName"],
                "title": record["title"],
                "status": record["resvStatus"],
                "approval_status": record["resvCheckStatus"],
                "kind": record["resvKind"],
                "start": record["resvTime"]["startTime"],
                "end": record["resvTime"]["endTime"],
                "sites": record["siteVoList"].as_array().into_iter().flatten().map(|site| json!({
                    "site_uuid": site["uuid"],
                    "site_name": site["siteName"],
                    "site_type": site["siteType"]
                })).collect::<Vec<_>>(),
                "order_created": record["orderCreate"],
                "order_number": record["orderNo"],
                "order_status": record["orderStatus"],
                "paid_amount_cents": paid_amount_cents,
                "paid_amount_yuan": paid_amount_cents.map(|cents| cents as f64 / 100.0),
                "payment_deadline": record["paymentDeadline"],
                "cancel_before_start_minutes": record["cancelBeforeStartTime"],
                "refund_deadline": record["refundDeadline"]
            })
        })
        .collect::<Vec<_>>();
    json!({"count": records.len(), "records": records})
}

fn is_bookable(status: &Value) -> bool {
    status["reserveStatus"].as_str() == Some("Y")
}

fn reason(status: &Value) -> Option<String> {
    status["reserveStatusReason"]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn find_site<'a>(sites: &'a [Value], args: &Value) -> Result<&'a Value> {
    let uuid = args["site_uuid"].as_str().filter(|value| !value.is_empty());
    let name = args["site_name"].as_str().filter(|value| !value.is_empty());
    if uuid.is_none() && name.is_none() {
        bail!("site_uuid or site_name is required for booking");
    }
    let matches = sites
        .iter()
        .filter(|site| {
            uuid.is_none_or(|wanted| site["uuid"].as_str() == Some(wanted))
                && name.is_none_or(|wanted| {
                    site["siteName"]
                        .as_str()
                        .is_some_and(|actual| actual == wanted)
                })
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [site] => Ok(site),
        [] => bail!("the requested site is not in the availability result"),
        _ => bail!("site_name is ambiguous; use the exact site_uuid from sports.availability"),
    }
}

fn find_session<'a>(site: &'a Value, start: &str, end: &str) -> Option<&'a Value> {
    if !is_bookable(&site["reserveStatus"]) {
        return None;
    }
    site["sessionVo"].as_array()?.iter().find(|session| {
        is_bookable(&session["reserveStatus"])
            && normalized_date_time(session, "beginDate", "beginTime").as_deref() == Some(start)
            && normalized_date_time(session, "endDate", "endTime").as_deref() == Some(end)
    })
}

fn normalized_date_time(value: &Value, date_key: &str, time_key: &str) -> Option<String> {
    let date = value[date_key]
        .as_str()
        .map(str::to_owned)
        .or_else(|| value[date_key].as_i64().map(|date| date.to_string()))?;
    let time = value[time_key].as_str()?;
    Some(format!(
        "{}-{}-{} {}",
        date.get(0..4)?,
        date.get(4..6)?,
        date.get(6..8)?,
        time.get(0..5)?
    ))
}

fn nonempty_string(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
}

fn payment_mode(pay_type: Option<i64>) -> &'static str {
    match pay_type {
        Some(2) => "PAY_OFFLINE",
        _ => "PAY_ONLINE",
    }
}

fn validate_payment_mode(pay_type: &str) -> Result<()> {
    match pay_type {
        "PAY_ONLINE" | "PAY_OFFLINE" | "PAY_CARD" => Ok(()),
        _ => bail!("pay_type must be PAY_ONLINE, PAY_OFFLINE or PAY_CARD"),
    }
}

fn required(args: &Value, key: &str) -> Result<String> {
    match &args[key] {
        Value::String(value) if !value.trim().is_empty() => Ok(value.clone()),
        Value::Number(value) => Ok(value.to_string()),
        _ => bail!("missing required argument: {key}"),
    }
}

fn optional_number(args: &Value, key: &str, default: i64) -> i64 {
    args[key]
        .as_i64()
        .or_else(|| args[key].as_str().and_then(|value| value.parse().ok()))
        .unwrap_or(default)
}

fn validate_date(date: &str) -> Result<()> {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").context("date must be YYYY-MM-DD")?;
    Ok(())
}

fn validate_window(start: &str, end: &str) -> Result<()> {
    if start.len() != 16 || end.len() != 16 {
        bail!("start and end must be YYYY-MM-DD HH:MM in Beijing time");
    }
    let start_at = chrono::NaiveDateTime::parse_from_str(start, "%Y-%m-%d %H:%M")
        .context("start must be YYYY-MM-DD HH:MM in Beijing time")?;
    let end_at = chrono::NaiveDateTime::parse_from_str(end, "%Y-%m-%d %H:%M")
        .context("end must be YYYY-MM-DD HH:MM in Beijing time")?;
    if end_at <= start_at {
        bail!("end must be after start");
    }
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap());
    if start_at < now.naive_local() {
        bail!("cannot book a sports session in the past");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token() -> StoredToken {
        StoredToken {
            token: "x".into(),
            refresh_token: "y".into(),
        }
    }

    #[test]
    fn sports_token_sidecar_roundtrip_shape() {
        let raw = serde_json::to_string(&token()).unwrap();
        let decoded: StoredToken = serde_json::from_str(&raw).unwrap();
        assert_eq!(decoded.token, "x");
        assert_eq!(decoded.refresh_token, "y");
    }

    #[test]
    fn sign_shape_is_complete() {
        let params = signature_params();
        assert_eq!(params[0].0, "appId");
        assert_eq!(params[1].0, "timeStamp");
        assert_eq!(params[2].0, "nonce");
        assert_eq!(params[2].1.len(), 32);
        assert_eq!(params[3].0, "sign");
        assert_eq!(params[3].1.len(), 32);
    }

    #[test]
    fn session_time_parser_uses_platform_layout() {
        let session = json!({
            "beginDate":20261106,
            "beginTime":"18:00",
            "endDate":20261106,
            "endTime":"20:00"
        });
        assert_eq!(
            normalized_date_time(&session, "beginDate", "beginTime").as_deref(),
            Some("2026-11-06 18:00")
        );
    }

    #[test]
    fn availability_is_concise_and_filters_not_open_sessions() {
        let availability = json!({
            "data": [{
                "uuid": "site-1",
                "siteName": "Court 1",
                "siteType": "DEV",
                "reserveStatus": {"reserveStatus": "Y", "reserveStatusReason": ""},
                "formRuleVo": {"formUuid": "form-1"},
                "reserveInfo": [{"primaryUserPhone": "private"}],
                "sessionVo": [{
                    "uuid": "session-open",
                    "beginDate": 20261106,
                    "beginTime": "18:00",
                    "endDate": 20261106,
                    "endTime": "19:00",
                    "sceneUseType": "SPORT_GROUP",
                    "allowUserNum": 2,
                    "resvUserNum": 1,
                    "reserveStatus": {"reserveStatus": "Y", "reserveStatusReason": ""},
                    "userFeeDetails": {"chargingUnitPrice": 3000, "payType": "PAY_ONLINE"}
                }, {
                    "uuid": "session-closed",
                    "beginDate": 20261106,
                    "beginTime": "19:00",
                    "endDate": 20261106,
                    "endTime": "20:00",
                    "reserveStatus": {
                        "reserveStatus": "N",
                        "reserveStatusReason": "未开放"
                    }
                }]
            }]
        });
        let normalized = normalize_availability(&availability, "2026-11-06", "scene-1", false);
        assert_eq!(normalized["count"], 1);
        assert_eq!(normalized["bookable_session_count"], 1);
        assert_eq!(
            normalized["sites"][0]["sessions"][0]["session_uuid"],
            "session-open"
        );
        assert_eq!(
            normalized["sites"][0]["sessions"].as_array().unwrap().len(),
            1
        );
        assert!(normalized.get("reserveInfo").is_none());
        assert!(!normalized.to_string().contains("private"));
    }

    #[test]
    fn booking_session_must_be_open() {
        let site = json!({
            "reserveStatus": {"reserveStatus": "Y"},
            "sessionVo": [{
                "beginDate": 20261106,
                "beginTime": "18:00",
                "endDate": 20261106,
                "endTime": "19:00",
                "reserveStatus": {"reserveStatus": "N"}
            }]
        });
        assert!(find_session(&site, "2026-11-06 18:00", "2026-11-06 19:00").is_none());
    }

    #[test]
    fn records_omit_member_identity_fields() {
        let value = json!({
            "data": [{
                "resvUuid": "reservation-1",
                "sceneName": "Tennis",
                "resvStatus": "RESV_END",
                "resvTime": {
                    "startTime": "2026-11-06 06:00:00",
                    "endTime": "2026-11-06 07:30:00"
                },
                "paidAmount": 18000,
                "userId": "private-user-id",
                "resvMemberVos": [{"name": "private-member"}],
                "siteVoList": [{"uuid": "site-1", "siteName": "Court 1", "siteType": "DEV"}]
            }]
        });
        let normalized = normalize_records(&value).to_string();
        assert!(normalized.contains("reservation-1"));
        assert!(!normalized.contains("private-user-id"));
        assert!(!normalized.contains("private-member"));
    }

    #[test]
    fn validates_booking_window() {
        assert!(validate_window("2099-01-01 08:00", "2099-01-01 10:00").is_ok());
        assert!(validate_window("2099-01-01 10:00", "2099-01-01 08:00").is_err());
        assert!(validate_window("bad", "2099-01-01 08:00").is_err());
    }
}
