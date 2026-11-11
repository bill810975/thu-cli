//! Read-only discovery for applications published by the Info service portal.
//!
//! The portal exposes more than three hundred application cards.  This module
//! keeps that live catalogue discoverable through one stable CLI surface and
//! statically inspects application landing pages without invoking the
//! application's own business endpoints.
use crate::session::Session;
use anyhow::{bail, Context, Result};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::OnceLock;
use tokio::task::JoinSet;
use url::Url;

const INFO_ROOT: &str = "https://info.tsinghua.edu.cn";
const CATALOG_URL: &str =
    "https://info.tsinghua.edu.cn/b/yyfw/vyyfwxx/info/portal_fg/common/getCyAppList/xsAll";
const APP_METADATA_URL: &str =
    "https://info.tsinghua.edu.cn/b/yyfw/vyyfwxx/info/portal_fg/teacher/getYyfwjs";
const APP_REDIRECT_URL: &str =
    "https://info.tsinghua.edu.cn/b/yyfw/vyyfwxx/info/portal_fg/common/onlineAppRedirect";
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
struct CatalogResponse {
    result: String,
    #[serde(default)]
    object: Vec<CatalogCategory>,
}

#[derive(Clone, Debug, Deserialize)]
struct CatalogCategory {
    #[serde(default)]
    ywlbmc: String,
    #[serde(default)]
    data: Vec<CatalogApp>,
}

#[derive(Clone, Debug, Deserialize)]
struct CatalogApp {
    yyfwid: String,
    #[serde(default)]
    yyfwmc: String,
    #[serde(default)]
    ywlbmc: Option<String>,
    #[serde(default)]
    sfsc: Option<String>,
    #[serde(default)]
    id: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HttpMethod {
    Get,
    Post,
    Unknown,
}

impl HttpMethod {
    fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Unknown => "UNKNOWN",
        }
    }
}

fn op(
    name: &str,
    description: &str,
    required: &[&str],
    optional: &[&str],
    upstream: &str,
) -> Value {
    json!({
        "name": name,
        "mutating": false,
        "description": description,
        "required": required,
        "optional": optional,
        "upstream": upstream
    })
}

pub fn operations() -> Value {
    json!([
        op(
            "service.categories",
            "List every live Info application category and its application count",
            &[],
            &[],
            "getCyAppList/xsAll"
        ),
        op(
            "service.list",
            "List the live Info application catalogue; all 300+ cards are included",
            &[],
            &["category", "query", "native_only", "page", "page_size"],
            "getCyAppList/xsAll"
        ),
        op(
            "service.info",
            "Read portal metadata and safe entry-point summaries for one application",
            &["yyfwid"],
            &[],
            "getYyfwjs"
        ),
        op(
            "service.resolve",
            "Resolve an application's SSO destinations without invoking its business APIs",
            &["yyfwid"],
            &[],
            "onlineAppRedirect"
        ),
        op(
            "service.discover",
            "Statically discover API candidates from one application's HTML and JavaScript",
            &["yyfwid"],
            &["max_scripts"],
            "authenticated landing page + script assets"
        ),
        op(
            "service.audit",
            "Audit portal metadata for all matching applications and optionally run static discovery",
            &[],
            &[
                "category",
                "query",
                "native_only",
                "limit",
                "deep",
                "max_scripts",
                "concurrency"
            ],
            "getYyfwjs + onlineAppRedirect + static assets"
        )
    ])
}

pub async fn execute(session: &Session, operation: &str, args: &Value) -> Result<Value> {
    match operation {
        "service.categories" => {
            let catalog = load_catalog(session).await?;
            Ok(json!({
                "total": catalog.iter().map(|category| category.data.len()).sum::<usize>(),
                "categories": catalog.iter().map(category_summary).collect::<Vec<_>>()
            }))
        }
        "service.list" => list_services(session, args).await,
        "service.info" => {
            let yyfwid = required_string(args, "yyfwid")?;
            service_info(session, &yyfwid).await
        }
        "service.resolve" => {
            let yyfwid = required_string(args, "yyfwid")?;
            let csrf = csrf_token(session)?;
            resolve_service(&session.client, &csrf, &yyfwid).await
        }
        "service.discover" => {
            let yyfwid = required_string(args, "yyfwid")?;
            let csrf = csrf_token(session)?;
            let max_scripts = bounded_usize(args, "max_scripts", 12, 1, 200)?;
            discover_service(&session.client, &csrf, &yyfwid, max_scripts).await
        }
        "service.audit" => audit_services(session, args).await,
        _ => bail!("Unknown service discovery operation: {operation}"),
    }
}

async fn load_catalog(session: &Session) -> Result<Vec<CatalogCategory>> {
    let csrf = csrf_token(session)?;
    let mut url = Url::parse(CATALOG_URL)?;
    url.query_pairs_mut().append_pair("_csrf", &csrf);
    let response = portal_request(session.client.get(url).header("Referer", portal_home())).await?;
    let parsed: CatalogResponse =
        serde_json::from_str(&response).context("Info service catalogue did not return JSON")?;
    if parsed.result != "success" {
        bail!("Info service catalogue rejected the request");
    }
    if parsed.object.is_empty() {
        bail!("Info service catalogue returned no categories");
    }
    Ok(parsed.object)
}

async fn list_services(session: &Session, args: &Value) -> Result<Value> {
    let categories = load_catalog(session).await?;
    let category_filter = optional_string(args, "category").map(|value| value.to_lowercase());
    let query = optional_string(args, "query").map(|value| value.to_lowercase());
    let native_only = args["native_only"].as_bool().unwrap_or(false);
    let page = bounded_usize(args, "page", 1, 1, usize::MAX)?;
    let page_size = bounded_usize(args, "page_size", 1000, 1, 1000)?;

    let mut items = Vec::new();
    for category in &categories {
        if category_filter
            .as_ref()
            .is_some_and(|filter| !category.ywlbmc.to_lowercase().contains(filter))
        {
            continue;
        }
        for app in &category.data {
            let adapters = native_adapters(&app.yyfwmc);
            if native_only && adapters.is_empty() {
                continue;
            }
            if query.as_ref().is_some_and(|needle| {
                !app.yyfwmc.to_lowercase().contains(needle)
                    && !app.yyfwid.to_lowercase().contains(needle)
            }) {
                continue;
            }
            items.push(app_json(&category.ywlbmc, app));
        }
    }

    let total = items.len();
    let offset = page.saturating_sub(1).saturating_mul(page_size);
    let page_items = items
        .into_iter()
        .skip(offset)
        .take(page_size)
        .collect::<Vec<_>>();
    Ok(json!({
        "portal_total": categories.iter().map(|category| category.data.len()).sum::<usize>(),
        "matching_total": total,
        "page": page,
        "page_size": page_size,
        "categories": categories.iter().map(category_summary).collect::<Vec<_>>(),
        "items": page_items
    }))
}

async fn service_info(session: &Session, yyfwid: &str) -> Result<Value> {
    validate_yyfwid(yyfwid)?;
    let csrf = csrf_token(session)?;
    let (metadata, metadata_error) = match app_metadata(&session.client, &csrf, yyfwid).await {
        Ok(metadata) => (metadata, Value::Null),
        Err(error) => (Value::Null, json!(error.to_string())),
    };
    let categories = load_catalog(session).await?;
    let app = categories
        .iter()
        .flat_map(|category| category.data.iter().map(move |app| (&category.ywlbmc, app)))
        .find(|(_, app)| app.yyfwid == yyfwid)
        .map(|(category, app)| app_json(category, app));
    Ok(json!({
        "yyfwid": yyfwid,
        "catalogue": app,
        "metadata": metadata,
        "metadata_error": metadata_error,
        "safe_portal_urls": {
            "information": format!("{INFO_ROOT}/f/info/portal_fg/teacher/yyfwxxappinfo?yyfwid={yyfwid}"),
            "entry": format!("{INFO_ROOT}/f/info/portal_fg/teacher/roamingToApp?yyfwid={yyfwid}&bz=app")
        },
        "note": "Portal entry URLs contain the application identifier only; SSO tickets are never returned."
    }))
}

async fn app_metadata(client: &Client, csrf: &str, yyfwid: &str) -> Result<Value> {
    let mut url = Url::parse(APP_METADATA_URL)?;
    url.query_pairs_mut()
        .append_pair("yyfwid", yyfwid)
        .append_pair("_csrf", csrf);
    let mut response = portal_request(
        client
            .post(url.clone())
            .header(
                "Content-Type",
                "application/x-www-form-urlencoded; charset=UTF-8",
            )
            .header("X-XSRF-TOKEN", csrf)
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Origin", INFO_ROOT)
            .header("Referer", portal_home())
            .body(""),
    )
    .await?;
    let mut value: Value =
        serde_json::from_str(&response).context("Application metadata did not return JSON")?;
    if value["result"] != "success" {
        response = portal_request(
            client
                .get(url)
                .header("X-XSRF-TOKEN", csrf)
                .header("X-Requested-With", "XMLHttpRequest")
                .header("Referer", portal_home()),
        )
        .await?;
        value =
            serde_json::from_str(&response).context("Application metadata did not return JSON")?;
    }
    if value["result"] != "success" {
        bail!(
            "Application metadata request was rejected: result={}, message={}",
            value["result"],
            value["msg"]
        );
    }
    Ok(value.get("object").cloned().unwrap_or(Value::Null))
}

async fn resolve_service(client: &Client, csrf: &str, yyfwid: &str) -> Result<Value> {
    validate_yyfwid(yyfwid)?;
    let resolved = resolve_raw(client, csrf, yyfwid).await?;
    Ok(json!({
        "yyfwid": yyfwid,
        "online": resolved.online,
        "roaming_destination": safe_url_summary(&resolved.roaming),
        "online_destination": resolved.next.as_ref().map(safe_url_summary),
        "note": "Raw SSO URLs and all query values are intentionally omitted."
    }))
}

async fn discover_service(
    client: &Client,
    csrf: &str,
    yyfwid: &str,
    max_scripts: usize,
) -> Result<Value> {
    validate_yyfwid(yyfwid)?;
    let resolved = resolve_raw(client, csrf, yyfwid).await?;
    let discovery = discover_resolved(client, &resolved, max_scripts).await?;
    Ok(json!({
        "yyfwid": yyfwid,
        "online": resolved.online,
        "sso_destination": safe_url_summary(&resolved.roaming),
        "online_destination": resolved.next.as_ref().map(safe_url_summary),
        "discovery": discovery,
        "safety": {
            "business_api_calls": 0,
            "endpoint_classification": "static_source_only",
            "write_candidates_are_reported_but_never_invoked": true
        }
    }))
}

async fn audit_services(session: &Session, args: &Value) -> Result<Value> {
    let categories = load_catalog(session).await?;
    let category_filter = optional_string(args, "category").map(|value| value.to_lowercase());
    let query = optional_string(args, "query").map(|value| value.to_lowercase());
    let native_only = args["native_only"].as_bool().unwrap_or(false);
    let limit = bounded_usize(args, "limit", 1000, 1, 1000)?;
    let deep = args["deep"].as_bool().unwrap_or(false);
    let max_scripts = bounded_usize(args, "max_scripts", 8, 1, 40)?;
    let concurrency = bounded_usize(args, "concurrency", 4, 1, 12)?;
    let csrf = csrf_token(session)?;

    let mut apps = Vec::new();
    'categories: for category in &categories {
        if category_filter
            .as_ref()
            .is_some_and(|filter| !category.ywlbmc.to_lowercase().contains(filter))
        {
            continue;
        }
        for app in &category.data {
            if native_only && native_adapters(&app.yyfwmc).is_empty() {
                continue;
            }
            if query.as_ref().is_some_and(|needle| {
                !app.yyfwmc.to_lowercase().contains(needle)
                    && !app.yyfwid.to_lowercase().contains(needle)
            }) {
                continue;
            }
            apps.push((category.ywlbmc.clone(), app.clone()));
            if apps.len() >= limit {
                break 'categories;
            }
        }
    }

    let client = session.client.clone();
    let mut results = Vec::with_capacity(apps.len());
    for chunk in apps.chunks(concurrency) {
        let mut tasks = JoinSet::new();
        for (category, app) in chunk.iter().cloned() {
            let client = client.clone();
            let csrf = csrf.clone();
            tasks.spawn(async move {
                audit_one(&client, &csrf, category, app, deep, max_scripts).await
            });
        }
        while let Some(result) = tasks.join_next().await {
            results.push(result.context("service discovery worker failed")??);
        }
    }
    results.sort_by(|left, right| {
        (
            left["category"].as_str(),
            left["name"].as_str(),
            left["yyfwid"].as_str(),
        )
            .cmp(&(
                right["category"].as_str(),
                right["name"].as_str(),
                right["yyfwid"].as_str(),
            ))
    });

    let succeeded = results
        .iter()
        .filter(|result| result["status"] == "ok")
        .count();
    Ok(json!({
        "portal_total": categories.iter().map(|category| category.data.len()).sum::<usize>(),
        "audited": results.len(),
        "succeeded": succeeded,
        "failed": results.len().saturating_sub(succeeded),
        "deep": deep,
        "results": results,
        "safety": {
            "business_api_calls": 0,
            "only_portal_metadata_sso_landing_and_static_assets_requested": true
        }
    }))
}

async fn audit_one(
    client: &Client,
    csrf: &str,
    category: String,
    app: CatalogApp,
    deep: bool,
    max_scripts: usize,
) -> Result<Value> {
    let mut status = "ok";
    let mut error = Value::Null;
    let mut metadata = Value::Null;
    let mut metadata_error = Value::Null;
    let mut resolution = Value::Null;
    let mut discovery = Value::Null;

    match app_metadata(client, csrf, &app.yyfwid).await {
        Ok(value) => metadata = value,
        Err(err) => {
            metadata_error = json!(err.to_string());
        }
    }

    match resolve_raw(client, csrf, &app.yyfwid).await {
        Ok(resolved) => {
            resolution = json!({
                "online": resolved.online,
                "roaming_destination": safe_url_summary(&resolved.roaming),
                "online_destination": resolved.next.as_ref().map(safe_url_summary)
            });
            if deep {
                match discover_resolved(client, &resolved, max_scripts).await {
                    Ok(value) => discovery = value,
                    Err(err) => {
                        status = "partial";
                        error = json!(err.to_string());
                    }
                }
            }
        }
        Err(err) => {
            status = "error";
            error = json!(err.to_string());
        }
    }

    Ok(json!({
        "yyfwid": app.yyfwid,
        "name": app.yyfwmc,
        "category": category,
        "native_adapters": native_adapters(&app.yyfwmc),
        "status": status,
        "error": error,
        "metadata": metadata,
        "metadata_error": metadata_error,
        "resolution": resolution,
        "discovery": discovery
    }))
}

async fn discover_resolved(
    client: &Client,
    resolved: &ResolvedService,
    max_scripts: usize,
) -> Result<Value> {
    let target = if resolved.online {
        let next = resolved
            .next
            .as_ref()
            .context("online application response did not contain its destination")?;
        let response = client
            .get(resolved.roaming.clone())
            .header(
                "Referer",
                "https://info.tsinghua.edu.cn/f/info/portal_fg/student/yyfwxxindex",
            )
            .send()
            .await
            .context("online application SSO callback failed")?;
        response_text(response).await?;
        next.clone()
    } else {
        resolved.roaming.clone()
    };
    discover_landing(client, target, max_scripts).await
}

struct ResolvedService {
    online: bool,
    roaming: Url,
    next: Option<Url>,
}

async fn resolve_raw(client: &Client, csrf: &str, yyfwid: &str) -> Result<ResolvedService> {
    let mut url = Url::parse(APP_REDIRECT_URL)?;
    url.query_pairs_mut()
        .append_pair("yyfwid", yyfwid)
        .append_pair("machine", "p")
        .append_pair("_csrf", csrf);
    let response = portal_request(
        client
            .post(url)
            .header(
                "Content-Type",
                "application/x-www-form-urlencoded; charset=UTF-8",
            )
            .header("X-XSRF-TOKEN", csrf)
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Origin", INFO_ROOT)
            .header("Referer", portal_home())
            .body(""),
    )
    .await?;
    let value: Value =
        serde_json::from_str(&response).context("SSO resolution did not return JSON")?;
    if value["result"] != "success" {
        bail!("Information portal rejected SSO resolution for this application");
    }
    let online = value["object"]["isOnline"].as_str() == Some("yes");
    let roaming = parse_service_url(
        value["object"]["roamingurl"]
            .as_str()
            .context("SSO response did not contain a roaming destination")?,
    )
    .context("SSO response did not contain a usable roaming destination")?;
    let next = value["object"]["nexturl"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .map(parse_service_url)
        .transpose()
        .context("SSO response contained an invalid online destination")?;
    Ok(ResolvedService {
        online,
        roaming,
        next,
    })
}

async fn discover_landing(client: &Client, url: Url, max_scripts: usize) -> Result<Value> {
    validate_school_url(&url)?;
    let response = client
        .get(url.clone())
        .header(
            "Referer",
            "https://info.tsinghua.edu.cn/f/info/portal_fg/student/yyfwxxindex",
        )
        .send()
        .await
        .context("application landing page request failed")?;
    let (final_url, html) = response_text(response).await?;
    validate_school_url(&final_url)?;
    let mut endpoint_map = BTreeMap::new();
    for endpoint in extract_endpoints(&html, final_url.as_str(), "landing-html") {
        let method = endpoint["method"].as_str().unwrap_or("UNKNOWN");
        let key = format!("{}:{}", method, endpoint["path"]);
        endpoint_map.entry(key).or_insert(endpoint);
    }

    let (title, script_urls, inline_count) = {
        let document = Html::parse_document(&html);
        let title_selector = Selector::parse("title").expect("static selector");
        let script = Selector::parse("script").expect("static selector");
        let module_preload =
            Selector::parse("link[rel~='modulepreload'][href]").expect("static selector");
        let forms = Selector::parse("form[action]").expect("static selector");

        for form in document.select(&forms) {
            let Some(action) = form.value().attr("action") else {
                continue;
            };
            let method = match form
                .value()
                .attr("method")
                .unwrap_or("GET")
                .to_ascii_uppercase()
                .as_str()
            {
                "POST" => HttpMethod::Post,
                "GET" => HttpMethod::Get,
                _ => HttpMethod::Unknown,
            };
            if let Some(endpoint) = endpoint_json(action, method, "html-form", final_url.as_str()) {
                let key = format!("{}:{}", method.as_str(), endpoint["path"]);
                endpoint_map.entry(key).or_insert(endpoint);
            }
        }

        let mut script_urls = BTreeSet::new();
        let mut inline_count = 0;
        for element in document.select(&script) {
            if let Some(src) = element.value().attr("src") {
                if let Some(url) = resolve_asset_url(&final_url, src) {
                    if validate_school_url(&url).is_ok() {
                        script_urls.insert(url);
                    }
                }
            } else {
                inline_count += 1;
            }
        }
        for element in document.select(&module_preload) {
            if let Some(href) = element.value().attr("href") {
                if let Some(url) = resolve_asset_url(&final_url, href) {
                    if validate_school_url(&url).is_ok() {
                        script_urls.insert(url);
                    }
                }
            }
        }
        let title = document
            .select(&title_selector)
            .next()
            .map(|node| node.text().collect::<String>().trim().to_owned());
        (title, script_urls, inline_count)
    };

    let mut script_queue = VecDeque::from_iter(script_urls);
    let mut visited_scripts = BTreeSet::new();
    let mut scanned_scripts = Vec::new();
    let mut script_failures = Vec::new();
    while let Some(script_url) = script_queue.pop_front() {
        if scanned_scripts.len() + script_failures.len() >= max_scripts {
            break;
        }
        if !visited_scripts.insert(script_url.clone()) {
            continue;
        }
        let response = client
            .get(script_url.clone())
            .header("Referer", final_url.as_str())
            .send()
            .await;
        let Ok(response) = response else {
            script_failures.push(json!({"host":script_url.host_str(),"path":script_url.path(),"error":"request failed"}));
            continue;
        };
        let (resolved_url, source) = match response_text(response).await {
            Ok(value) => value,
            Err(error) => {
                script_failures.push(json!({"host":script_url.host_str(),"path":script_url.path(),"error":error.to_string()}));
                continue;
            }
        };
        scanned_scripts.push(json!({
            "host": resolved_url.host_str(),
            "path": resolved_url.path()
        }));
        for endpoint in extract_endpoints(&source, resolved_url.as_str(), "static-script") {
            let method = endpoint["method"].as_str().unwrap_or("UNKNOWN");
            let key = format!("{}:{}", method, endpoint["path"]);
            endpoint_map.entry(key).or_insert(endpoint);
        }
        for asset in extract_script_assets(&source, &resolved_url) {
            if visited_scripts.len() + script_queue.len() < max_scripts.saturating_mul(4) {
                script_queue.push_back(asset);
            }
        }
    }

    let scripts_scanned = scanned_scripts.len();
    let endpoints = endpoint_map.into_values().collect::<Vec<_>>();
    let read_candidates = endpoints
        .iter()
        .filter(|endpoint| endpoint["classification"] == "read_candidate")
        .count();
    let write_candidates = endpoints
        .iter()
        .filter(|endpoint| endpoint["classification"] == "write_candidate")
        .count();
    let unclassified_candidates = endpoints
        .iter()
        .filter(|endpoint| endpoint["classification"] == "unclassified_candidate")
        .count();
    Ok(json!({
        "landing": safe_url_summary(&final_url),
        "title": title,
        "scripts_discovered": scripts_scanned + script_failures.len(),
        "scripts_scanned": scripts_scanned,
        "scripts": scanned_scripts,
        "inline_scripts_ignored": inline_count,
        "script_failures": script_failures,
        "endpoint_count": endpoints.len(),
        "read_candidates": read_candidates,
        "write_candidates": write_candidates,
        "unclassified_candidates": unclassified_candidates,
        "endpoints": endpoints,
        "note": "Endpoints were extracted from HTML and JavaScript only. No discovered business endpoint was invoked."
    }))
}

fn extract_script_assets(source: &str, base: &Url) -> Vec<Url> {
    static SCRIPT_LITERAL: OnceLock<Regex> = OnceLock::new();
    let pattern = SCRIPT_LITERAL.get_or_init(|| {
        Regex::new(r#"["'`]([^"'`\r\n]{1,512}\.(?:js|mjs)(?:\?[^"'`\r\n]*)?)["'`]"#)
            .expect("static script asset regex")
    });
    let mut assets = BTreeSet::new();
    for capture in pattern.captures_iter(source) {
        let Some(value) = capture.get(1).map(|value| value.as_str()) else {
            continue;
        };
        if value.starts_with("data:") || value.starts_with("javascript:") {
            continue;
        }
        let Ok(url) = base.join(value) else {
            continue;
        };
        if validate_school_url(&url).is_ok() {
            assets.insert(url);
        }
    }
    assets.into_iter().collect()
}

fn extract_endpoints(source: &str, base: &str, source_name: &str) -> Vec<Value> {
    static URL_LITERAL: OnceLock<Regex> = OnceLock::new();
    let pattern = URL_LITERAL.get_or_init(|| {
        Regex::new(r#"["'`]((?:https?://|//|/)[A-Za-z0-9_?&=./:${}%~+@-]+)["'`]"#)
            .expect("static endpoint regex")
    });
    let mut endpoints = BTreeMap::new();
    for capture in pattern.captures_iter(source) {
        let Some(found) = capture.get(1) else {
            continue;
        };
        let raw = found.as_str();
        if !looks_like_endpoint(raw) {
            continue;
        }
        let before_start = char_boundary_at_or_before(source, found.start().saturating_sub(180));
        let after_end = char_boundary_at_or_after(source, (found.end() + 20).min(source.len()));
        let before = &source[before_start..found.start()];
        let after = &source[found.end()..after_end];
        let method = infer_method(before, after);
        if let Some(endpoint) = endpoint_json(raw, method, source_name, base) {
            endpoints
                .entry(format!("{}:{}", method.as_str(), endpoint["path"]))
                .or_insert(endpoint);
        }
    }
    endpoints.into_values().collect()
}

fn char_boundary_at_or_before(value: &str, mut index: usize) -> usize {
    while index > 0 && !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn char_boundary_at_or_after(value: &str, mut index: usize) -> usize {
    while index < value.len() && !value.is_char_boundary(index) {
        index += 1;
    }
    index
}

fn endpoint_json(raw: &str, method: HttpMethod, source: &str, base: &str) -> Option<Value> {
    let path = normalize_endpoint(raw, base)?;
    let lower = path.to_ascii_lowercase();
    let mutation_word = [
        "add", "save", "submit", "delete", "update", "bind", "apply", "pay", "order", "upload",
        "create", "modify", "edit", "remove", "cancel", "reset", "recharge", "approve", "audit",
        "sign", "send", "import", "insert", "commit", "checkout", "withdraw",
    ]
    .iter()
    .any(|word| lower.contains(word));
    let classification = if method == HttpMethod::Post || mutation_word {
        "write_candidate"
    } else if method == HttpMethod::Get {
        "read_candidate"
    } else {
        "unclassified_candidate"
    };
    Some(json!({
        "path": path,
        "method": method.as_str(),
        "classification": classification,
        "source": source
    }))
}

fn normalize_endpoint(raw: &str, base: &str) -> Option<String> {
    let raw = raw.replace("&amp;", "&");
    let parsed = if raw.starts_with("//") {
        Url::parse(&format!("https:{raw}")).ok()?
    } else if raw.starts_with('/') || raw.starts_with("./") || raw.starts_with("../") {
        Url::parse(base).ok()?.join(&raw).ok()?
    } else {
        let url = Url::parse(&raw).ok()?;
        validate_school_url(&url).ok()?;
        url
    };
    if !is_school_host(parsed.host_str().unwrap_or_default()) {
        return None;
    }
    let mut output = parsed.path().to_owned();
    if let Some(query) = parsed.query() {
        let keys = query
            .split('&')
            .filter_map(|part| part.split('=').next())
            .filter(|key| !key.is_empty())
            .collect::<BTreeSet<_>>();
        if !keys.is_empty() {
            output.push('?');
            output.push_str(
                &keys
                    .into_iter()
                    .map(|key| format!("{key}=<value>"))
                    .collect::<Vec<_>>()
                    .join("&"),
            );
        }
    }
    Some(output)
}

fn looks_like_endpoint(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    if [
        ".js", ".mjs", ".css", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".ico", ".woff", ".woff2",
        ".ttf", ".map", ".webp",
    ]
    .iter()
    .any(|suffix| {
        lower
            .split(['?', '#'])
            .next()
            .unwrap_or("")
            .ends_with(suffix)
    }) {
        return false;
    }
    [
        "/b/",
        "/api/",
        "/api-",
        "/ic-web/",
        "/rest/",
        "/rpc/",
        "/graphql",
        "/model-api/",
        "/mark-",
        "/mark/",
        "/server/",
        "/gateway/",
        "/prod-api/",
        "/v1/",
        "/v2/",
        "/v3/",
        "/query",
        "/search",
        "/list",
        "/report/",
        "/user/",
        "/auth/",
        "/login",
        "/sso/",
        ".do",
        ".action",
        ".jsp",
        ".php",
        ".json",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn infer_method(before: &str, after: &str) -> HttpMethod {
    let context = format!(
        "{} {}",
        before.to_ascii_lowercase(),
        after.to_ascii_lowercase()
    );
    if [
        "type:\"post\"",
        "type: \"post\"",
        "type:'post'",
        "type: 'post'",
        "method:\"post\"",
        "method: \"post\"",
        "method:'post'",
        "method: 'post'",
        ".post(",
        "axios.post",
        "method=\"post\"",
    ]
    .iter()
    .any(|marker| context.contains(marker))
    {
        HttpMethod::Post
    } else if [
        "type:\"get\"",
        "type: \"get\"",
        "method:\"get\"",
        "method: \"get\"",
        ".get(",
        "axios.get",
        "fetch(",
        "method=\"get\"",
    ]
    .iter()
    .any(|marker| context.contains(marker))
    {
        HttpMethod::Get
    } else {
        HttpMethod::Unknown
    }
}

fn resolve_asset_url(base: &Url, value: &str) -> Option<Url> {
    if value.starts_with("data:") || value.starts_with("javascript:") {
        return None;
    }
    base.join(value).ok()
}

fn parse_service_url(value: &str) -> Result<Url> {
    let value = value.replace("&amp;", "&");
    let url = if value.starts_with("//") {
        Url::parse(&format!("https:{value}"))?
    } else {
        Url::parse(&value)
            .or_else(|_| Url::parse(INFO_ROOT)?.join(&value))
            .context("invalid destination URL")?
    };
    validate_school_url(&url)?;
    Ok(url)
}

fn safe_url_summary(url: &Url) -> Value {
    let sensitive_names = ["ticket", "token", "auth", "session", "sign", "key", "code"];
    let query_keys = url
        .query_pairs()
        .map(|(key, _)| key.into_owned())
        .collect::<BTreeSet<_>>();
    let sensitive_removed = query_keys
        .iter()
        .filter(|key| {
            let lower = key.to_ascii_lowercase();
            sensitive_names.iter().any(|marker| lower.contains(marker))
        })
        .cloned()
        .collect::<Vec<_>>();
    json!({
        "scheme": url.scheme(),
        "host": url.host_str(),
        "port": url.port(),
        "path": url.path(),
        "query_keys": query_keys,
        "sensitive_query_keys_removed": sensitive_removed,
        "raw_url_omitted": true
    })
}

fn validate_school_url(url: &Url) -> Result<()> {
    if !matches!(url.scheme(), "http" | "https") {
        bail!("refusing non-HTTP application destination");
    }
    if !is_school_host(url.host_str().unwrap_or_default()) {
        bail!("refusing application destination outside tsinghua.edu.cn");
    }
    Ok(())
}

fn is_school_host(host: &str) -> bool {
    host == "tsinghua.edu.cn" || host.ends_with(".tsinghua.edu.cn")
}

async fn portal_request(request: reqwest::RequestBuilder) -> Result<String> {
    let response = request.send().await.context("Info portal request failed")?;
    let (_, text) = response_text(response).await?;
    Ok(text)
}

async fn response_text(response: reqwest::Response) -> Result<(Url, String)> {
    let status = response.status();
    let final_url = response.url().clone();
    if !status.is_success() {
        if matches!(status.as_u16(), 401 | 403) {
            bail!(
                "application authentication expired; request returned HTTP {}",
                status.as_u16()
            );
        }
        bail!("application request returned HTTP {}", status.as_u16());
    }
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut response = response;
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .context("reading application response failed")?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            bail!("application response exceeded the safe four-megabyte limit");
        }
        bytes.extend_from_slice(&chunk);
    }
    let text = if content_type.contains("gb") {
        encoding_rs::GBK.decode(&bytes).0.into_owned()
    } else {
        String::from_utf8(bytes.clone())
            .unwrap_or_else(|_| encoding_rs::GBK.decode(&bytes).0.into_owned())
    };
    if content_type.contains("html")
        && (text.contains("sm2publicKey") || text.contains("do/off/ui/auth/login"))
    {
        bail!("application session expired or requires authentication");
    }
    Ok((final_url, text))
}

fn csrf_token(session: &Session) -> Result<String> {
    session
        .cookie_value("info.tsinghua.edu.cn", "XSRF-TOKEN")?
        .filter(|value| !value.trim().is_empty())
        .context("information-portal CSRF cookie missing; authentication required")
}

fn required_string(args: &Value, key: &str) -> Result<String> {
    args[key]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .with_context(|| format!("Missing string argument: {key}"))
}

fn optional_string(args: &Value, key: &str) -> Option<String> {
    args[key]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn bounded_usize(args: &Value, key: &str, default: usize, min: usize, max: usize) -> Result<usize> {
    let value = match &args[key] {
        Value::Null => default,
        Value::Number(number) => number
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .context(format!("{key} must be a positive integer"))?,
        Value::String(value) if !value.is_empty() => value
            .parse::<usize>()
            .with_context(|| format!("{key} must be a positive integer"))?,
        _ => bail!("{key} must be a positive integer"),
    };
    if value < min || value > max {
        bail!("{key} must be between {min} and {max}");
    }
    Ok(value)
}

fn validate_yyfwid(yyfwid: &str) -> Result<()> {
    if yyfwid.is_empty()
        || yyfwid.len() > 64
        || !yyfwid
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        bail!("yyfwid contains unsupported characters");
    }
    Ok(())
}

fn portal_home() -> &'static str {
    "https://info.tsinghua.edu.cn/f/info/portal_fg/student/yyfwxxindex"
}

fn category_summary(category: &CatalogCategory) -> Value {
    json!({
        "name": category.ywlbmc,
        "count": category.data.len()
    })
}

fn app_json(category: &str, app: &CatalogApp) -> Value {
    let adapters = native_adapters(&app.yyfwmc);
    json!({
        "yyfwid": app.yyfwid,
        "name": app.yyfwmc,
        "category": app.ywlbmc.as_deref().unwrap_or(category),
        "portal_id": app.id,
        "sfsc": app.sfsc,
        "native_adapters": adapters,
        "coverage": if adapters.is_empty() { "portal-discoverable" } else { "native-adapter-present" }
    })
}

fn native_adapters(name: &str) -> Vec<&'static str> {
    if name.contains("网络学堂") {
        vec!["learn.*"]
    } else if name.contains("本科生选课") {
        vec!["course.*"]
    } else if name == "课表" {
        vec!["course.schedule", "schedule.list"]
    } else if name.contains("全部成绩") || name.contains("中文成绩单") {
        vec!["info.report"]
    } else if name.contains("体育场馆预约") {
        vec!["sports.*"]
    } else if name.contains("教学楼研讨间预约") {
        vec!["classroom-reservation.*"]
    } else if name.contains("算力平台") {
        vec!["easycompute.*", "paratera.*"]
    } else if name.contains("临时来访报备") {
        vec!["visit.*"]
    } else if name.contains("学生访客") || name.contains("亲友来访") {
        vec!["visit.thos-*", "thos.*"]
    } else if name.contains("校园卡综合服务") {
        vec!["card.*"]
    } else if name.contains("网络账号管理") {
        vec!["network.*"]
    } else if name.contains("DeepSeek服务") {
        vec!["model.chat", "info.model-token"]
    } else if name.contains("代码托管服务") {
        vec!["gitlab.*"]
    } else if name.contains("自助报账") {
        vec!["wsbx.*"]
    } else if name.contains("教室状况查看") {
        vec!["classroom.*"]
    } else if name.contains("电子发票") {
        vec!["invoice.list", "invoice.pdf"]
    } else if name.contains("清华邮箱") {
        vec!["mail.send"]
    } else if name.contains("研读间/研讨间预约") {
        vec!["library.room-*"]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_operations_are_read_only() {
        let operations = operations();
        let operations = operations.as_array().unwrap();
        assert_eq!(operations.len(), 6);
        assert!(operations
            .iter()
            .all(|operation| operation["mutating"] == false));
    }

    #[test]
    fn extracts_read_and_write_candidates() {
        let source = r#"
            $.ajax({type:"GET", url:"/api/query/list?token=secret"});
            $.post("/api/application/add", {name:"x"});
        "#;
        let endpoints = extract_endpoints(source, INFO_ROOT, "fixture");
        assert!(endpoints.iter().any(|endpoint| endpoint["path"]
            == "/api/query/list?token=<value>"
            && endpoint["classification"] == "read_candidate"));
        assert!(endpoints
            .iter()
            .any(|endpoint| endpoint["path"] == "/api/application/add"
                && endpoint["classification"] == "write_candidate"));
    }

    #[test]
    fn safe_url_summary_never_returns_values() {
        let url =
            Url::parse("https://example.tsinghua.edu.cn/b/app?ticket=private&mode=read").unwrap();
        let summary = safe_url_summary(&url);
        let serialized = summary.to_string();
        assert!(!serialized.contains("private"));
        assert!(!serialized.contains("read\""));
        assert_eq!(summary["sensitive_query_keys_removed"][0], "ticket");
    }

    #[test]
    fn native_adapters_cover_known_live_names() {
        assert_eq!(native_adapters("本科生选课"), vec!["course.*"]);
        assert_eq!(
            native_adapters("DeepSeek服务"),
            vec!["model.chat", "info.model-token"]
        );
        assert_eq!(
            native_adapters("教学楼研讨间预约"),
            vec!["classroom-reservation.*"]
        );
        assert_eq!(
            native_adapters("算力平台"),
            vec!["easycompute.*", "paratera.*"]
        );
        assert_eq!(
            native_adapters("学生访客（无身份证人员）预约"),
            vec!["visit.thos-*", "thos.*"]
        );
        assert!(native_adapters("专利预审").is_empty());
    }

    #[test]
    fn rejects_non_campus_destinations() {
        let url = Url::parse("https://example.com/api").unwrap();
        assert!(validate_school_url(&url).is_err());
    }

    #[test]
    fn endpoint_context_handles_multibyte_text() {
        let source = "中文中文中文中文中文$.post('/api/application/add')";
        let endpoints = extract_endpoints(source, INFO_ROOT, "fixture");
        assert!(endpoints
            .iter()
            .any(|endpoint| endpoint["path"] == "/api/application/add"));
    }

    #[test]
    fn extracts_recursive_module_assets() {
        let base = Url::parse("https://example.tsinghua.edu.cn/assets/main.js").unwrap();
        let source = r#"
            import("./chunk-login.js");
            export { default } from '/assets/chunk-query.js';
            const icon = "/assets/icon.svg";
        "#;
        let assets = extract_script_assets(source, &base);
        assert!(assets
            .iter()
            .any(|url| url.path() == "/assets/chunk-login.js"));
        assert!(assets
            .iter()
            .any(|url| url.path() == "/assets/chunk-query.js"));
        assert!(!assets.iter().any(|url| url.path().ends_with(".svg")));
    }
}
