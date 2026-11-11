//! Tsinghua classroom-reservation platform adapter.
//!
//! The current platform at `jxlytj.tsinghua.edu.cn` signs every request,
//! authenticates through unified identity, and may encrypt JSON responses.
//! This module keeps its rotating token in a private sidecar and exposes
//! read-only discovery plus explicitly confirmed reservation actions.

use aes::Aes256;
use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use chrono::{Datelike, Local};
use cipher::{Block, BlockDecrypt, KeyInit};
use md5::{Digest, Md5};
use rand::Rng;
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

use crate::{
    auth::{self, LoginOptions},
    credential,
    session::Session,
};

const ROOT: &str = "https://jxlytj.tsinghua.edu.cn";
const API_ROOT: &str = "https://jxlytj.tsinghua.edu.cn/site";
const BLANK_PAGE: &str = "https://jxlytj.tsinghua.edu.cn/blank";
const SIGN_APP_ID: &str = "1497016617475903488";
const SIGN_KEY: &str = "57325972627c40bd8c77296d39293705";
const SIGN_CHARS: &[u8] = b"ABCDEFGHJKMNPQRSTWXYZabcdefhijkmnprstwxyz0123456789";
const TOKEN_SIDECAR: &str = "jxlytj-auth";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct StoredToken {
    #[serde(default)]
    token: String,
    #[serde(default, rename = "refreshToken")]
    refresh_token: String,
}

struct ClassroomApi {
    client: Client,
    token: String,
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
            "classroom-reservation.auth-status",
            false,
            "Current classroom-reservation account and private session state",
            &[],
            &[],
            "jxlytj.tsinghua.edu.cn/site/system/login/getLoginUser"
        ),
        operation(
            "classroom-reservation.scenes",
            false,
            "List classroom and meeting-room reservation scenes",
            &[],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/site/scene/list"
        ),
        operation(
            "classroom-reservation.scene-detail",
            false,
            "Read one reservation scene, opening rules and booking modes",
            &["uuid"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/site/scene/detail"
        ),
        operation(
            "classroom-reservation.site-types",
            false,
            "List room/site types for one reservation scene",
            &["scene_uuid"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/site/siteType"
        ),
        operation(
            "classroom-reservation.sites",
            false,
            "List selectable campuses, buildings, or rooms for one reservation scene",
            &["scene_uuid"],
            &["site_type", "site_uuid"],
            "jxlytj.tsinghua.edu.cn/site/api/site/choose"
        ),
        operation(
            "classroom-reservation.room-dictionary",
            false,
            "Read equipment or property tags attached to one room",
            &["room_uuid"],
            &["type_code"],
            "jxlytj.tsinghua.edu.cn/site/site/room/dict/list"
        ),
        operation(
            "classroom-reservation.recommend",
            false,
            "Read platform-recommended reservation windows",
            &[],
            &["query", "payload"],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/recommend"
        ),
        operation(
            "classroom-reservation.current",
            false,
            "List current-day reservation availability using the exact frontend payload",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/current/page"
        ),
        operation(
            "classroom-reservation.current-periods",
            false,
            "List available periods for a current-day room search",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/current/period"
        ),
        operation(
            "classroom-reservation.current-detail",
            false,
            "Read one current-day availability result and its bookable periods",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/current/detail"
        ),
        operation(
            "classroom-reservation.cross-day",
            false,
            "List cross-day room availability",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/cross/page"
        ),
        operation(
            "classroom-reservation.cross-day-detail",
            false,
            "Read one cross-day availability result",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/cross/detail"
        ),
        operation(
            "classroom-reservation.custom-current",
            false,
            "List custom-time same-day room availability",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/custom/current/page"
        ),
        operation(
            "classroom-reservation.custom-current-periods",
            false,
            "List bookable periods for custom-time same-day availability",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/custom/current/period"
        ),
        operation(
            "classroom-reservation.custom-current-detail",
            false,
            "Read one custom-time same-day availability result",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/custom/current/detail"
        ),
        operation(
            "classroom-reservation.custom-cross-day",
            false,
            "List custom-time cross-day room availability",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/custom/cross/page"
        ),
        operation(
            "classroom-reservation.custom-cross-day-detail",
            false,
            "Read one custom-time cross-day availability result",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/custom/cross/detail"
        ),
        operation(
            "classroom-reservation.custom-seats",
            false,
            "Read available seats for one custom-time room slot",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/custom/current/seat"
        ),
        operation(
            "classroom-reservation.quick-current",
            false,
            "Find the next quick same-day reservation",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/quick/current"
        ),
        operation(
            "classroom-reservation.quick-cross-day",
            false,
            "Find the next quick cross-day reservation",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/quick/cross"
        ),
        operation(
            "classroom-reservation.quick-periods",
            false,
            "Read quick-reservation periods",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/quick/period"
        ),
        operation(
            "classroom-reservation.quick-page",
            false,
            "List quick-reservation matches",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/quick/page"
        ),
        operation(
            "classroom-reservation.commonly-used",
            false,
            "Read the account's commonly used rooms",
            &[],
            &["query"],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/commonlyUsed"
        ),
        operation(
            "classroom-reservation.equipment",
            false,
            "Read the room equipment dictionary",
            &[],
            &["query"],
            "jxlytj.tsinghua.edu.cn/site/system/dev/dict/equipment/dropDown"
        ),
        operation(
            "classroom-reservation.task",
            false,
            "Read a completed reservation task",
            &[],
            &["query"],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/complete/task"
        ),
        operation(
            "classroom-reservation.groups",
            false,
            "Read the account's reservation groups",
            &[],
            &["query"],
            "jxlytj.tsinghua.edu.cn/site/user/favorite/groups/details"
        ),
        operation(
            "classroom-reservation.workflow-process",
            false,
            "List reservation approval workflow instances",
            &[],
            &["query"],
            "jxlytj.tsinghua.edu.cn/site/workflow/process/list"
        ),
        operation(
            "classroom-reservation.workflow-info",
            false,
            "Read one reservation approval workflow instance",
            &["id"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/workflow/process/{id}"
        ),
        operation(
            "classroom-reservation.workflow-brief",
            false,
            "Read a reservation approval workflow definition summary",
            &["id"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/workflow/process/brief/{id}"
        ),
        operation(
            "classroom-reservation.workflow-define",
            false,
            "Read a reservation form/workflow definition",
            &["id"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/workflow/define/detail/{id}"
        ),
        operation(
            "classroom-reservation.records",
            false,
            "List current classroom reservation records",
            &[],
            &["payload"],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/reserveRecord"
        ),
        operation(
            "classroom-reservation.history",
            false,
            "List historical classroom reservation records",
            &[],
            &["payload"],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/historyRecord"
        ),
        operation(
            "classroom-reservation.detail",
            false,
            "Read one reservation detail",
            &["id"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/details"
        ),
        operation(
            "classroom-reservation.copy-source",
            false,
            "Read a previous reservation as a new-booking source",
            &["id"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/copyResv"
        ),
        operation(
            "classroom-reservation.credit",
            false,
            "Read the current user's reservation credit or quota",
            &[],
            &["scene_uuid", "page_size", "page_num"],
            "jxlytj.tsinghua.edu.cn/site/api/user/getUserCredit"
        ),
        operation(
            "classroom-reservation.lock-config",
            false,
            "Read reservation locking and hold configuration",
            &[],
            &["payload"],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/getLockConfig"
        ),
        operation(
            "classroom-reservation.system-config",
            false,
            "Read classroom reservation system configuration",
            &[],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/system/config"
        ),
        operation(
            "classroom-reservation.read",
            false,
            "Call one allowlisted read-only classroom-reservation endpoint",
            &["method", "path"],
            &["query", "payload"],
            "jxlytj.tsinghua.edu.cn allowlisted reservation API catalogue"
        ),
        operation(
            "classroom-reservation.lock",
            true,
            "Lock a specific room/period with a reviewed frontend payload",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/lockSite"
        ),
        operation(
            "classroom-reservation.unlock",
            true,
            "Release a room/period lock with a reviewed frontend payload",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/unLockSite"
        ),
        operation(
            "classroom-reservation.book",
            true,
            "Submit one current/fixed-period classroom reservation payload",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/addReserve"
        ),
        operation(
            "classroom-reservation.book-custom",
            true,
            "Submit one custom-time classroom reservation payload",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/custom/addReserve"
        ),
        operation(
            "classroom-reservation.cancel",
            true,
            "Cancel one classroom reservation by exact server-shaped payload",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/cancelReserve"
        ),
        operation(
            "classroom-reservation.cancel-period",
            true,
            "Cancel one period of a classroom reservation",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/cancelReserve/period"
        ),
        operation(
            "classroom-reservation.cancel-batch",
            true,
            "Cancel selected classroom reservations in one payload",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/cancelReserveBatch"
        ),
        operation(
            "classroom-reservation.detail-update",
            true,
            "Update an existing classroom reservation from a reviewed payload",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/details/update"
        ),
        operation(
            "classroom-reservation.end-early",
            true,
            "End an active classroom reservation early",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/endEarly"
        ),
        operation(
            "classroom-reservation.extend",
            true,
            "Extend an active classroom reservation using a reviewed payload",
            &["payload"],
            &[],
            "jxlytj.tsinghua.edu.cn/site/api/reserve/extend"
        )
    ])
}

pub async fn execute(session: &Session, operation: &str, args: &Value) -> Result<Value> {
    let mut api = ClassroomApi::open(session).await?;
    match operation {
        "classroom-reservation.auth-status" => {
            let user = api.user().await?;
            Ok(json!({
                "authenticated": true,
                "account": user["account"],
                "name": user["name"],
                "identity": user["ident"],
                "admin": user["adminUser"],
                "session_storage": "private mode-600 sidecar"
            }))
        }
        "classroom-reservation.scenes" => api.get("/api/site/scene/list", &[]).await,
        "classroom-reservation.scene-detail" => {
            api.get(
                "/api/site/scene/detail",
                &[("uuid", required_string(args, "uuid")?)],
            )
            .await
        }
        "classroom-reservation.site-types" => {
            api.get(
                "/api/site/siteType",
                &[("sceneUuid", required_string(args, "scene_uuid")?)],
            )
            .await
        }
        "classroom-reservation.sites" => {
            let site_type =
                optional_string(args, "site_type").unwrap_or_else(|| "CAMPUS".to_owned());
            let site_uuid = optional_string(args, "site_uuid").unwrap_or_default();
            api.get(
                "/api/site/choose",
                &[
                    ("sceneUuid", required_string(args, "scene_uuid")?),
                    ("siteType", site_type),
                    ("siteUuid", site_uuid),
                ],
            )
            .await
        }
        "classroom-reservation.room-dictionary" => {
            let room_uuid = required_string(args, "room_uuid")?;
            let type_code =
                optional_string(args, "type_code").unwrap_or_else(|| "ROOM_EQUIPMENT".to_owned());
            api.get(
                "/site/room/dict/list",
                &[("roomUuid", room_uuid), ("typeCode", type_code)],
            )
            .await
        }
        "classroom-reservation.recommend" => {
            let default_query = json!({"pageSize": 10, "pageNum": 1});
            let query = args
                .get("query")
                .or_else(|| args.get("payload"))
                .filter(|value| value.is_object())
                .unwrap_or(&default_query);
            api.get(
                "/api/reserve/recommend",
                &query_pairs_to_borrowed(Some(query))?,
            )
            .await
        }
        "classroom-reservation.current" => {
            api.post(
                "/api/reserve/current/page",
                required_object(args, "payload")?,
            )
            .await
        }
        "classroom-reservation.current-periods" => {
            api.post(
                "/api/reserve/current/period",
                required_object(args, "payload")?,
            )
            .await
        }
        "classroom-reservation.current-detail" => {
            api.post(
                "/api/reserve/current/detail",
                required_object(args, "payload")?,
            )
            .await
        }
        "classroom-reservation.cross-day" => {
            api.post("/api/reserve/cross/page", required_object(args, "payload")?)
                .await
        }
        "classroom-reservation.cross-day-detail" => {
            api.post(
                "/api/reserve/cross/detail",
                required_object(args, "payload")?,
            )
            .await
        }
        "classroom-reservation.custom-current" => {
            api.post(
                "/api/reserve/custom/current/page",
                required_object(args, "payload")?,
            )
            .await
        }
        "classroom-reservation.custom-current-periods" => {
            api.post(
                "/api/reserve/custom/current/period",
                required_object(args, "payload")?,
            )
            .await
        }
        "classroom-reservation.custom-current-detail" => {
            api.post(
                "/api/reserve/custom/current/detail",
                required_object(args, "payload")?,
            )
            .await
        }
        "classroom-reservation.custom-cross-day" => {
            api.post(
                "/api/reserve/custom/cross/page",
                required_object(args, "payload")?,
            )
            .await
        }
        "classroom-reservation.custom-cross-day-detail" => {
            api.post(
                "/api/reserve/custom/cross/detail",
                required_object(args, "payload")?,
            )
            .await
        }
        "classroom-reservation.custom-seats" => {
            api.post(
                "/api/reserve/custom/current/seat",
                required_object(args, "payload")?,
            )
            .await
        }
        "classroom-reservation.quick-current" => {
            api.post(
                "/api/reserve/quick/current",
                required_object(args, "payload")?,
            )
            .await
        }
        "classroom-reservation.quick-cross-day" => {
            api.post(
                "/api/reserve/quick/cross",
                required_object(args, "payload")?,
            )
            .await
        }
        "classroom-reservation.quick-periods" => {
            api.post(
                "/api/reserve/quick/period",
                required_object(args, "payload")?,
            )
            .await
        }
        "classroom-reservation.quick-page" => {
            api.post("/api/reserve/quick/page", required_object(args, "payload")?)
                .await
        }
        "classroom-reservation.commonly-used" => {
            api.get(
                "/api/reserve/commonlyUsed",
                &query_pairs_to_borrowed(args.get("query"))?,
            )
            .await
        }
        "classroom-reservation.equipment" => {
            let default_query = json!({"typeCode": "ROOM_EQUIPMENT"});
            let query = args
                .get("query")
                .filter(|value| value.is_object())
                .unwrap_or(&default_query);
            api.get(
                "/system/dev/dict/equipment/dropDown",
                &query_pairs_to_borrowed(Some(query))?,
            )
            .await
        }
        "classroom-reservation.task" => {
            api.get(
                "/api/reserve/complete/task",
                &query_pairs_to_borrowed(args.get("query"))?,
            )
            .await
        }
        "classroom-reservation.groups" => {
            let default_query = json!({"pageSize": 100, "pageNum": 1, "groupName": ""});
            let query = args
                .get("query")
                .filter(|value| value.is_object())
                .unwrap_or(&default_query);
            api.get(
                "/user/favorite/groups/details",
                &query_pairs_to_borrowed(Some(query))?,
            )
            .await
        }
        "classroom-reservation.workflow-process" => {
            api.get(
                "/workflow/process/list",
                &query_pairs_to_borrowed(args.get("query"))?,
            )
            .await
        }
        "classroom-reservation.workflow-info" => {
            let path = format!(
                "/workflow/process/{}",
                encoded_path_segment(&required_string(args, "id")?)
            );
            api.get(&path, &[]).await
        }
        "classroom-reservation.workflow-brief" => {
            let path = format!(
                "/workflow/process/brief/{}",
                encoded_path_segment(&required_string(args, "id")?)
            );
            api.get(&path, &[]).await
        }
        "classroom-reservation.workflow-define" => {
            let path = format!(
                "/workflow/define/detail/{}",
                encoded_path_segment(&required_string(args, "id")?)
            );
            api.get(&path, &[]).await
        }
        "classroom-reservation.records" => {
            api.post(
                "/api/reserve/reserveRecord",
                &object_or_default(
                    args,
                    "payload",
                    json!({
                        "orderStatus": "",
                        "refundStatus": "",
                        "resvStatus": "",
                        "resvExtendStatus": "",
                        "resvCheckStatus": "",
                        "pageSize": 20,
                        "pageNum": 1,
                        "orderItems": "gmt_create",
                        "orderRule": "desc",
                        "containsActivity": false
                    }),
                ),
            )
            .await
        }
        "classroom-reservation.history" => {
            let now = Local::now();
            let default_payload = json!({
                "year": now.format("%Y").to_string(),
                "range": {
                    "startTime": format!("{}-01-01", now.year()),
                    "endTime": now.format("%Y-%m-%d").to_string()
                },
                "pageSize": 20,
                "pageNum": 1
            });
            api.post(
                "/api/reserve/historyRecord",
                &object_or_default(args, "payload", default_payload),
            )
            .await
        }
        "classroom-reservation.detail" => {
            api.get(
                "/api/reserve/details",
                &[("id", required_string(args, "id")?)],
            )
            .await
        }
        "classroom-reservation.copy-source" => {
            api.get(
                "/api/reserve/copyResv",
                &[("id", required_string(args, "id")?)],
            )
            .await
        }
        "classroom-reservation.credit" => {
            if let Some(scene_uuid) = optional_string(args, "scene_uuid") {
                api.get("/api/user/getSceneCredit", &[("sceneUuid", scene_uuid)])
                    .await
            } else {
                let page_size = args["page_size"].as_u64().unwrap_or(10).to_string();
                let page_num = args["page_num"].as_u64().unwrap_or(1).to_string();
                api.get(
                    "/api/user/getUserCredit",
                    &[("pageSize", page_size), ("pageNum", page_num)],
                )
                .await
            }
        }
        "classroom-reservation.lock-config" => {
            let default_payload = json!({});
            api.post(
                "/api/reserve/getLockConfig",
                args.get("payload")
                    .filter(|value| value.is_object())
                    .unwrap_or(&default_payload),
            )
            .await
        }
        "classroom-reservation.system-config" => api.get("/api/reserve/system/config", &[]).await,
        "classroom-reservation.read" => read_operation(&mut api, args).await,
        "classroom-reservation.lock"
        | "classroom-reservation.unlock"
        | "classroom-reservation.book"
        | "classroom-reservation.book-custom"
        | "classroom-reservation.cancel"
        | "classroom-reservation.cancel-period"
        | "classroom-reservation.cancel-batch"
        | "classroom-reservation.detail-update"
        | "classroom-reservation.end-early"
        | "classroom-reservation.extend" => {
            let path = match operation {
                "classroom-reservation.lock" => "/api/reserve/lockSite",
                "classroom-reservation.unlock" => "/api/reserve/unLockSite",
                "classroom-reservation.book" => "/api/reserve/addReserve",
                "classroom-reservation.book-custom" => "/api/reserve/custom/addReserve",
                "classroom-reservation.cancel" => "/api/reserve/cancelReserve",
                "classroom-reservation.cancel-period" => "/api/reserve/cancelReserve/period",
                "classroom-reservation.cancel-batch" => "/api/reserve/cancelReserveBatch",
                "classroom-reservation.detail-update" => "/api/reserve/details/update",
                "classroom-reservation.end-early" => "/api/reserve/endEarly",
                "classroom-reservation.extend" => "/api/reserve/extend",
                _ => unreachable!(),
            };
            api.post(path, required_object(args, "payload")?).await
        }
        _ => bail!("unknown classroom-reservation operation: {operation}"),
    }
}

impl ClassroomApi {
    async fn open(session: &Session) -> Result<Self> {
        let stored = load_token(session)?;
        if !stored.token.is_empty() {
            let api = Self {
                client: session.client.clone(),
                token: stored.token,
            };
            if let Ok(user) = api.user_raw().await {
                if response_ok(&user) {
                    return Ok(api);
                }
            }
            if !stored.refresh_token.is_empty() {
                if let Ok(value) = api
                    .raw(
                        Method::POST,
                        "/system/login/refreshToken",
                        &[],
                        Some(&json!({"refreshToken": stored.refresh_token})),
                        None,
                    )
                    .await
                {
                    if response_ok(&value) {
                        let next = store_from_value(session, &value)?;
                        let refreshed = Self {
                            client: session.client.clone(),
                            token: next.token,
                        };
                        if let Ok(user) = refreshed.user_raw().await {
                            if response_ok(&user) {
                                return Ok(refreshed);
                            }
                        }
                    }
                }
            }
        }
        classroom_login(session).await
    }

    async fn user(&self) -> Result<Value> {
        let value = self.user_raw().await?;
        checked(&value)?;
        Ok(value["data"].clone())
    }

    async fn user_raw(&self) -> Result<Value> {
        self.raw(
            Method::GET,
            "/system/login/getLoginUser",
            &[],
            None,
            Some(&self.token),
        )
        .await
    }

    async fn get(&self, path: &str, params: &[(&str, String)]) -> Result<Value> {
        let value = self
            .raw(Method::GET, path, params, None, Some(&self.token))
            .await?;
        checked(&value)?;
        Ok(value)
    }

    async fn post(&self, path: &str, body: &Value) -> Result<Value> {
        let value = self
            .raw(Method::POST, path, &[], Some(body), Some(&self.token))
            .await?;
        checked(&value)?;
        Ok(value)
    }

    async fn raw(
        &self,
        method: Method,
        path: &str,
        params: &[(&str, String)],
        body: Option<&Value>,
        token: Option<&str>,
    ) -> Result<Value> {
        validate_api_path(path)?;
        let mut url = Url::parse(&format!("{API_ROOT}{path}"))?;
        {
            let mut query = url.query_pairs_mut();
            for (key, value) in signature_params() {
                query.append_pair(key, &value);
            }
            for (key, value) in params {
                query.append_pair(key, value);
            }
        }
        let mut request = self
            .client
            .request(method, url)
            .header("Language-Set", "CN")
            .header("Accept", "application/json, text/plain, */*")
            .header("Referer", format!("{ROOT}/"));
        if let Some(token) = token {
            request = request.header("token", token);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .await
            .context("classroom-reservation request failed")?;
        let status = response.status();
        let text = response
            .text()
            .await
            .context("reading classroom-reservation response")?;
        if !status.is_success() {
            bail!("classroom-reservation returned HTTP {status}");
        }
        decode_response(&text)
    }
}

async fn classroom_login(session: &Session) -> Result<ClassroomApi> {
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

    let unsigned = ClassroomApi {
        client: session.client.clone(),
        token: String::new(),
    };
    let address = unsigned
        .raw(
            Method::GET,
            "/cas/address",
            &[
                ("redirectUrl", BLANK_PAGE.to_owned()),
                ("queryParam", BLANK_PAGE.to_owned()),
                ("typeCode", String::new()),
            ],
            None,
            None,
        )
        .await?;
    let address = checked(&address)?["data"]
        .as_str()
        .ok_or_else(|| anyhow!("classroom-reservation login address is missing"))?
        .to_owned();
    validate_login_address(&address)?;

    let response = session
        .auth_client
        .get(&address)
        .send()
        .await
        .context("starting classroom-reservation SSO")?
        .error_for_status()
        .context("classroom-reservation SSO returned an HTTP error")?;
    let entry = response
        .text()
        .await
        .context("reading classroom-reservation SSO response")?;
    let callback = auth::destination_login(&session.auth_client, &entry, &options).await?;
    let callback = session
        .auth_client
        .get(callback)
        .send()
        .await
        .context("completing classroom-reservation SSO")?
        .error_for_status()
        .context("classroom-reservation SSO callback returned an HTTP error")?
        .url()
        .clone();
    let uni_token = callback
        .query_pairs()
        .find(|(key, _)| key == "uniToken")
        .map(|(_, value)| value.into_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow!("classroom-reservation SSO callback did not provide a one-time token")
        })?;

    let exchange = unsigned
        .raw(
            Method::POST,
            "/cas/token",
            &[],
            Some(&json!({
                "platForm": "CAS",
                "client": "PC",
                "token": uni_token
            })),
            None,
        )
        .await?;
    let exchange = checked(&exchange)?;
    let stored = store_from_value(session, exchange)?;
    let api = ClassroomApi {
        client: session.client.clone(),
        token: stored.token,
    };
    let user = api.user().await?;
    if user["account"].as_str() != Some(username.as_str()) {
        bail!("classroom-reservation session belongs to a different identity");
    }
    Ok(api)
}

async fn read_operation(api: &mut ClassroomApi, args: &Value) -> Result<Value> {
    let method = required_string(args, "method")?.to_ascii_uppercase();
    let path = required_string(args, "path")?;
    if !read_allowed(&method, &path) {
        bail!("This classroom-reservation endpoint/method is not on the read-only allowlist");
    }
    let params = query_pairs(args.get("query"), "query")?;
    let body = args.get("payload").filter(|value| value.is_object());
    let method = Method::from_bytes(method.as_bytes()).context("invalid HTTP method")?;
    let params = params.into_iter().collect::<Vec<_>>();
    let value = api
        .raw(
            method,
            &path,
            &params
                .iter()
                .map(|(key, value)| (key.as_str(), value.clone()))
                .collect::<Vec<_>>(),
            body,
            Some(&api.token),
        )
        .await?;
    checked(&value)?;
    Ok(value)
}

fn read_allowed(method: &str, path: &str) -> bool {
    let path = path.trim_start_matches('/');
    if method == "GET" {
        return path == "api/site/scene/list"
            || path == "api/site/scene/detail"
            || path == "api/site/siteType"
            || path == "api/site/choose"
            || path == "site/room/dict/list"
            || path == "api/reserve/details"
            || path == "api/reserve/copyResv"
            || path == "api/reserve/recommend"
            || path == "api/user/getUserCredit"
            || path == "api/user/getSceneCredit"
            || path == "api/reserve/system/config"
            || path == "api/reserve/record/quota"
            || path == "api/reserve/person"
            || path == "api/reserve/leader"
            || path == "api/reserve/commonlyUsed"
            || path == "api/reserve/complete/task"
            || path == "receipt/reserve/getReceiptInfo"
            || path == "receipt/reserve/details"
            || path == "system/dev/dict/equipment/dropDown"
            || path == "user/favorite/groups/details"
            || path.starts_with("workflow/process/")
            || path.starts_with("workflow/define/detail/")
            || path.starts_with("api/org/")
            || path.starts_with("api/ms/help/")
            || path.starts_with("api/app/");
    }
    if method != "POST" {
        return false;
    }
    [
        "api/reserve/current/page",
        "api/reserve/current/period",
        "api/reserve/current/detail",
        "api/reserve/cross/page",
        "api/reserve/cross/detail",
        "api/reserve/custom/current/page",
        "api/reserve/custom/current/period",
        "api/reserve/custom/current/detail",
        "api/reserve/custom/current/seat",
        "api/reserve/custom/cross/page",
        "api/reserve/custom/cross/detail",
        "api/reserve/quick/current",
        "api/reserve/quick/cross",
        "api/reserve/quick/period",
        "api/reserve/quick/page",
        "api/reserve/reserveRecord",
        "api/reserve/historyRecord",
        "resv/instr/getBySiteIdAndType",
        "api/reserve/getLockConfig",
        "api/reserve/system/config",
    ]
    .contains(&path)
}

fn signature_params() -> Vec<(&'static str, String)> {
    let time_stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string();
    let mut rng = rand::thread_rng();
    let nonce = (0..32)
        .map(|_| SIGN_CHARS[rng.gen_range(0..SIGN_CHARS.len())] as char)
        .collect::<String>();
    let source = format!("appId={SIGN_APP_ID}&nonce={nonce}&timeStamp={time_stamp}&key={SIGN_KEY}");
    let sign = format!("{:x}", Md5::digest(source.as_bytes()));
    vec![
        ("appId", SIGN_APP_ID.to_owned()),
        ("timeStamp", time_stamp),
        ("nonce", nonce),
        ("sign", sign),
    ]
}

fn decode_response(text: &str) -> Result<Value> {
    let raw = text.trim();
    let candidate = if raw.starts_with('"') {
        serde_json::from_str::<String>(raw).context("invalid encrypted response wrapper")?
    } else {
        raw.to_owned()
    };
    if candidate.trim_start().starts_with('{') || candidate.trim_start().starts_with('[') {
        return serde_json::from_str(&candidate).context("invalid classroom-reservation JSON");
    }
    let ciphertext = base64::engine::general_purpose::STANDARD
        .decode(candidate.trim())
        .context("classroom-reservation response is neither JSON nor base64 ciphertext")?;
    let plaintext = aes_cbc_decrypt(&ciphertext, SIGN_KEY.as_bytes(), b"0000000000000000")?;
    serde_json::from_slice(&plaintext).context("invalid decrypted classroom-reservation JSON")
}

fn aes_cbc_decrypt(ciphertext: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>> {
    if ciphertext.is_empty() || !ciphertext.len().is_multiple_of(16) {
        bail!("classroom-reservation ciphertext has an invalid block length")
    }
    if iv.len() != 16 {
        bail!("classroom-reservation AES IV has an invalid length")
    }
    let cipher = Aes256::new_from_slice(key).context("invalid classroom-reservation AES key")?;
    let mut previous: [u8; 16] = iv.try_into().expect("checked AES IV length");
    let mut output = Vec::with_capacity(ciphertext.len());
    for chunk in ciphertext.as_chunks::<16>().0 {
        let encrypted = *chunk;
        let mut block = Block::<Aes256>::clone_from_slice(chunk);
        cipher.decrypt_block(&mut block);
        for index in 0..16 {
            output.push(block[index] ^ previous[index]);
        }
        previous = encrypted;
    }
    let padding = *output
        .last()
        .ok_or_else(|| anyhow!("classroom-reservation plaintext is empty"))?
        as usize;
    if padding == 0 || padding > 16 || padding > output.len() {
        bail!("classroom-reservation response has invalid Iso10126 padding")
    }
    output.truncate(output.len() - padding);
    Ok(output)
}

fn checked(value: &Value) -> Result<&Value> {
    if response_ok(value) {
        return Ok(value);
    }
    if matches!(
        value["code"].as_i64(),
        Some(300 | 301 | 401 | 1030002 | 1130001 | 1130002 | 1130008 | 1130009)
    ) {
        bail!("classroom-reservation session expired")
    }
    bail!(
        "classroom-reservation rejected the request: {}",
        value["message"]
            .as_str()
            .unwrap_or("provider returned an unsuccessful response")
    )
}

fn response_ok(value: &Value) -> bool {
    value["code"].as_i64() == Some(0) && value["success"].as_bool() != Some(false)
}

fn store_from_value(session: &Session, value: &Value) -> Result<StoredToken> {
    let token = value["data"]["token"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("classroom-reservation login returned no access token"))?
        .to_owned();
    let stored = StoredToken {
        token,
        refresh_token: value["data"]["refreshToken"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
    };
    session.write_private_sidecar(
        TOKEN_SIDECAR,
        &serde_json::to_string(&stored).context("serializing classroom-reservation session")?,
    )?;
    Ok(stored)
}

fn load_token(session: &Session) -> Result<StoredToken> {
    let Some(raw) = session.read_private_sidecar(TOKEN_SIDECAR)? else {
        return Ok(StoredToken::default());
    };
    serde_json::from_str(&raw)
        .context("classroom-reservation private session metadata is malformed")
}

fn validate_login_address(address: &str) -> Result<()> {
    let address = Url::parse(address)?;
    if address.scheme() != "https"
        || address.host_str() != Some("jxlytj.tsinghua.edu.cn")
        || address.path() != "/site/authcenter/toLoginPage"
    {
        bail!("classroom-reservation returned an unexpected SSO address")
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
        bail!("unsafe or malformed classroom-reservation API path")
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

fn object_or_default(args: &Value, key: &str, default: Value) -> Value {
    args.get(key)
        .filter(|value| value.is_object())
        .cloned()
        .unwrap_or(default)
}

fn query_pairs(value: Option<&Value>, key: &str) -> Result<Vec<(String, String)>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("{key} must be a JSON object"))?;
    object
        .iter()
        .map(|(key, value)| {
            let value = match value {
                Value::String(value) => value.clone(),
                Value::Bool(_) | Value::Number(_) => value.to_string(),
                Value::Null => String::new(),
                _ => bail!("{key}.{key} must be a scalar"),
            };
            Ok((key.clone(), value))
        })
        .collect()
}

fn query_pairs_to_borrowed(value: Option<&Value>) -> Result<Vec<(&str, String)>> {
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
            Ok((key.as_str(), value))
        })
        .collect()
}

fn encoded_path_segment(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_params_have_expected_shape() {
        let params = signature_params();
        assert_eq!(params[0], ("appId", SIGN_APP_ID.to_owned()));
        assert_eq!(params[2].0, "nonce");
        assert_eq!(params[2].1.len(), 32);
        assert_eq!(params[3].0, "sign");
        assert_eq!(params[3].1.len(), 32);
    }

    #[test]
    fn iso10126_validation_rejects_bad_padding() {
        assert!(aes_cbc_decrypt(b"not-a-block", SIGN_KEY.as_bytes(), b"0000000000000000").is_err());
    }

    #[test]
    fn encrypted_response_roundtrip_matches_frontend_padding() {
        let encrypted = "3JGc48D5HfIC7EFvH3j/zR8R9Mb8Vx0pSZSesv4sEpQ=";
        let value = decode_response(&format!("\"{encrypted}\"")).unwrap();
        assert_eq!(value["data"]["ok"], true);
    }

    #[test]
    fn read_allowlist_refuses_writes() {
        assert!(read_allowed("GET", "api/reserve/recommend"));
        assert!(read_allowed("GET", "api/site/choose"));
        assert!(read_allowed("POST", "api/reserve/custom/cross/page"));
        assert!(read_allowed("POST", "resv/instr/getBySiteIdAndType"));
        assert!(read_allowed("POST", "api/reserve/current/page"));
        assert!(!read_allowed("POST", "api/reserve/lockSite"));
        assert!(!read_allowed("POST", "api/reserve/addReserve"));
        assert!(!read_allowed("POST", "api/reserve/extend"));
        assert!(!read_allowed("POST", "api/reserve/endEarly"));
    }

    #[test]
    fn plain_json_is_not_decrypted() {
        assert_eq!(
            decode_response(r#"{"code":0,"data":{"ok":true}}"#).unwrap()["data"]["ok"],
            true
        );
    }

    #[test]
    fn only_school_login_address_is_accepted() {
        assert!(validate_login_address(
            "https://jxlytj.tsinghua.edu.cn/site/authcenter/toLoginPage?x=1"
        )
        .is_ok());
        assert!(validate_login_address("https://example.com/site/authcenter/toLoginPage").is_err());
    }

    #[test]
    fn root_constant_is_school_host() {
        assert_eq!(
            Url::parse(ROOT).unwrap().host_str(),
            Some("jxlytj.tsinghua.edu.cn")
        );
    }
}
