//! App-only network features from THU Info utils/washer.ts and webApi.ts.
use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde_json::{json, Value};
use std::time::{Duration, Instant};
const ROOT: &str = "https://app.cs.tsinghua.edu.cn";
const XIAOLAN: &str = "https://wash-ltd-thu.aajax.top";
const ORG: &str = "67ce4044ba854c556508830e";
fn own_operations() -> Value {
    Value::Array([
 ("washer-jieli-buildings",false,"Jieli washer building list","",""),
 ("washer-jieli-floors",false,"Jieli washer status plus app location metadata","building",""),
 ("washer-haile-buildings",false,"Haile buildings near the two upstream campus coordinates","",""),
 ("washer-haile-floors",false,"Haile washer/dryer/shoe-washer status","building",""),
 ("washer-xiaolan-buildings",false,"Xiaolan building list","",""),
 ("washer-xiaolan-floors",false,"Xiaolan rooms and device status","building",""),
 ("schedule-sync-send",true,"Interactive SignalR schedule transfer to another app (requires peer)","match_id,input","timeout_seconds"),
 ("schedule-sync-receive",true,"Interactive SignalR receive to a new local file (requires peer)","match_id,output","timeout_seconds"),
 ("calendar-export",false,"Export explicit event JSON to a new RFC5545 ICS file","input,output",""),
 ].into_iter().map(|(name,mutating,description,required,optional)|json!({"name":name,"mutating":mutating,"description":description,"required":required.split(',').filter(|s|!s.is_empty()).collect::<Vec<_>>(),"optional":optional.split(',').filter(|s|!s.is_empty()).collect::<Vec<_>>(),"upstream":"thu-info-app@7930cad3f0d7243ad54df3ce4443b911113eef91 apps/thu-info-app/src/utils/{washer,webApi}.ts; calendar-export is native CLI addition"})).collect())
}
pub fn operations() -> Value {
    let mut own = own_operations().as_array().unwrap().clone();
    own.extend(
        crate::auxiliary::operations()
            .as_array()
            .unwrap()
            .iter()
            .cloned(),
    );
    Value::Array(own)
}
fn arg<'a>(a: &'a Value, k: &str) -> Result<&'a str> {
    a[k].as_str()
        .filter(|s| !s.is_empty())
        .with_context(|| format!("Missing nonempty string argument {k}"))
}
async fn response(r: reqwest::Response) -> Result<Value> {
    let status = r.status();
    if !status.is_success() {
        bail!("HTTP {}", status.as_u16())
    }
    r.json().await.context("Expected JSON from app service")
}
fn scalar(v: &Value) -> String {
    v.as_str().map(str::to_owned).unwrap_or_else(|| {
        if v.is_null() {
            String::new()
        } else {
            v.to_string()
        }
    })
}
fn xs(v: &Value) -> &str {
    let s = &v["deviceState"];
    if scalar(&s["isOnline"]) == "0" {
        return "offline";
    }
    if !s["fault"].is_null() && !matches!(scalar(&s["fault"]).as_str(), "0" | "false" | "") {
        return "error";
    }
    if scalar(&s["isOnline"]) != "1" {
        return "unknown";
    }
    match scalar(&s["runState"]).as_str() {
        "7" => "idle",
        "5" => "working",
        "1" => "standby",
        _ => "unknown",
    }
}
fn nonempty(v: &Value, fallback: &str) -> String {
    let s = scalar(v);
    if s.is_empty() {
        fallback.to_owned()
    } else {
        s
    }
}
fn parse_xiaolan(v: &Value, building: Option<&str>) -> Result<Value> {
    let b = v[ORG]["buildings"]
        .as_object()
        .context("Xiaolan response missing organization buildings")?;
    if let Some(id) = building {
        let facilities = b
            .get(id)
            .and_then(|x| x["facilities"].as_array())
            .context("Xiaolan response missing building facilities")?;
        let mut rooms = Vec::new();
        for f in facilities {
            let room_id = nonempty(
                &f["store"]["storeId"],
                &scalar(&f["storeDetail"]["storeId"]),
            );
            if room_id.is_empty() {
                bail!("Xiaolan room missing store ID")
            }
            let floor = scalar(&f["store"]["floor"]);
            let name = nonempty(
                &f["storeDetail"]["name"],
                &nonempty(
                    &f["store"]["opStoreName"],
                    &format!(
                        "洗衣房 {}",
                        if floor.is_empty() { &room_id } else { &floor }
                    ),
                ),
            );
            let mut washers = Vec::new();
            for d in f["devices"]
                .as_array()
                .context("Xiaolan room missing devices")?
            {
                let id = scalar(&d["deviceId"]);
                if id.is_empty() {
                    bail!("Xiaolan device missing ID")
                }
                let kind = match scalar(&d["type"]).as_str() {
                    "1" => "washer",
                    "2" => "dryer",
                    "3" => "washer-dryer",
                    "4" => "shoe-washer",
                    _ => "other",
                };
                washers.push(json!({"id":id,"name":nonempty(&d["deviceCode"],&id),"type":kind,"floor":name,"status":xs(d),"eta":-1,"update_time":v[ORG]["fetchedAt"],"estimated_complete_time":d["inUseBit"]["estimatedCompleteTime"]}));
            }
            washers.sort_by_key(|v| scalar(&v["name"]));
            rooms.push(json!({"id":room_id,"name":name,"floor":floor,"washers":washers}));
        }
        rooms.sort_by_key(|v| (scalar(&v["floor"]), scalar(&v["name"])));
        Ok(json!({"floors":rooms,"fetched_at":v[ORG]["fetchedAt"]}))
    } else {
        let mut result:Vec<_>=b.iter().map(|(id,x)|json!({"id":nonempty(&x["buildingId"],id),"name":nonempty(&x["name"],id),"provider":"xiaolan"})).collect();
        result.sort_by_key(|v| scalar(&v["name"]));
        Ok(json!(result))
    }
}
fn jieli_floors(v: &Value, locations: &Value) -> Result<Value> {
    let mut floors: std::collections::BTreeMap<String, Vec<Value>> =
        std::collections::BTreeMap::new();
    let eta_re = regex::Regex::new(r"剩余\D*(\d+)")?;
    for item in v["data"].as_array().context("Jieli device list missing")? {
        let status = scalar(&item["status"]);
        let eta = eta_re
            .captures(&status)
            .and_then(|c| c[1].parse::<u64>().ok())
            .unwrap_or(0);
        let state = if status.contains("待机") {
            "idle"
        } else if status.contains("工作") || status.contains("运转") {
            "working"
        } else {
            "error"
        };
        let mac = scalar(&item["macUnionCode"]);
        let mut parts = mac.split_whitespace();
        let kind = parts.next().unwrap_or("");
        let name = parts.next().unwrap_or("");
        let floor = scalar(&item["floorName"]);
        floors.entry(floor.clone()).or_default().push(json!({"type":kind,"name":name,"floor":floor,"status":state,"eta":eta,"status_text":status,"location":locations[name],"upstream":item}));
    }
    Ok(json!(floors
        .into_iter()
        .map(|(name, mut washers)| {
            washers.sort_by_key(|v| scalar(&v["name"]));
            json!({"name":name,"washers":washers})
        })
        .collect::<Vec<_>>()))
}
fn write_new(path: &str, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts
        .open(path)
        .context("Output must be a new file; existing files are not overwritten")?;
    let result = f.write_all(bytes).and_then(|_| f.sync_all());
    if let Err(e) = result {
        drop(f);
        let _ = std::fs::remove_file(path);
        return Err(e.into());
    };
    Ok(())
}
fn ical_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\r', "")
        .replace('\n', "\\n")
        .replace(';', "\\;")
        .replace(',', "\\,")
}
fn folded(line: &str) -> String {
    let mut out = String::new();
    let mut n = 0;
    for c in line.chars() {
        if n + c.len_utf8() > 73 {
            out.push_str("\r\n ");
            n = 1;
        }
        out.push(c);
        n += c.len_utf8();
    }
    out.push_str("\r\n");
    out
}
pub fn export_ics(events: &Value) -> Result<String> {
    let mut out = String::from(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//thu-cli//EN\r\nCALSCALE:GREGORIAN\r\n",
    );
    for (i, e) in events
        .as_array()
        .context("input must be an array of events")?
        .iter()
        .enumerate()
    {
        let start = chrono::DateTime::parse_from_rfc3339(arg(e, "start")?)
            .context("event start must be RFC3339 with timezone")?
            .with_timezone(&chrono::Utc);
        let end = chrono::DateTime::parse_from_rfc3339(arg(e, "end")?)
            .context("event end must be RFC3339 with timezone")?
            .with_timezone(&chrono::Utc);
        if end <= start {
            bail!("event end must be after start")
        }
        out.push_str("BEGIN:VEVENT\r\n");
        for line in [
            format!(
                "UID:{}",
                ical_escape(
                    e["uid"]
                        .as_str()
                        .unwrap_or(&format!("{}-{i}@thu-cli", start.timestamp()))
                )
            ),
            format!("DTSTAMP:{}", chrono::Utc::now().format("%Y%m%dT%H%M%SZ")),
            format!("DTSTART:{}", start.format("%Y%m%dT%H%M%SZ")),
            format!("DTEND:{}", end.format("%Y%m%dT%H%M%SZ")),
            format!("SUMMARY:{}", ical_escape(arg(e, "title")?)),
            format!(
                "LOCATION:{}",
                ical_escape(e["location"].as_str().unwrap_or(""))
            ),
            format!(
                "DESCRIPTION:{}",
                ical_escape(e["description"].as_str().unwrap_or(""))
            ),
        ] {
            out.push_str(&folded(&line));
        }
        out.push_str("END:VEVENT\r\n");
    }
    out.push_str("END:VCALENDAR\r\n");
    Ok(out)
}

pub async fn execute(c: &Client, op: &str, a: &Value) -> Result<Value> {
    if op.starts_with("water-") || op == "learnx-update" {
        return crate::auxiliary::execute(c, op, a).await;
    }
    match op {
        "washer-jieli-buildings" => {
            let v = response(
                c.post("https://api.cleverschool.cn/washapi4/device/tower")
                    .json(&json!({}))
                    .send()
                    .await?,
            )
            .await?;
            if !v["errorCode"].is_null() {
                bail!("Jieli reported an error")
            }
            Ok(json!(v["data"]
                .as_array()
                .context("Jieli building list missing")?
                .iter()
                .filter(|x| scalar(&x["value"]) != "0")
                .map(|x| json!({"id":scalar(&x["value"]),"name":x["text"],"provider":"jieli"}))
                .collect::<Vec<_>>()))
        }
        "washer-jieli-floors" => {
            let id = arg(a, "building")?;
            let v = response(
                c.post("https://api.cleverschool.cn/washapi4/device/status")
                    .json(&json!({"towerKey":id}))
                    .send()
                    .await?,
            )
            .await?;
            if !v["errorCode"].is_null() {
                bail!("Jieli reported an error")
            }
            let loc = async {
                response(
                    c.get(format!("{ROOT}/Api/JieliWashers"))
                        .query(&[("building", id)])
                        .send()
                        .await?,
                )
                .await
            }
            .await;
            let unavailable = loc.is_err();
            let loc = loc.unwrap_or_else(|_| json!({}));
            Ok(json!({"floors":jieli_floors(&v,&loc)?,"location_metadata_unavailable":unavailable}))
        }
        "washer-haile-buildings" => {
            let mut buildings = std::collections::BTreeMap::new();
            for (lng, lat) in [(116.32697, 40.00281), (116.3424247, 40.0313472)] {
                let v = response(
                    c.post("https://yshz-user.haier-ioc.com/position/nearPosition")
                        .json(&json!({"lng":lng,"lat":lat,"page":1,"pageSize":50}))
                        .send()
                        .await?,
                )
                .await?;
                if v["code"] != 0 {
                    bail!("Haile reported an error")
                }
                for x in v["data"]["items"]
                    .as_array()
                    .context("Haile items missing")?
                {
                    let n = scalar(&x["name"]);
                    if n.contains("清华") && !n.contains("中学") {
                        buildings.insert(
                            scalar(&x["id"]),
                            json!({"id":x["id"],"name":n,"provider":"haile"}),
                        );
                    }
                }
            }
            Ok(json!(buildings.into_values().collect::<Vec<_>>()))
        }
        "washer-haile-floors" => {
            let mut result = Vec::new();
            for category in ["00", "01", "02"] {
                let v=response(c.post("https://yshz-user.haier-ioc.com/position/deviceDetailPage").json(&json!({"positionId":arg(a,"building")?,"categoryCode":category,"page":1,"floorCode":"","pageSize":100})).send().await?).await?;
                if v["code"] != 0 {
                    bail!("Haile reported an error")
                }
                for d in v["data"]["items"]
                    .as_array()
                    .context("Haile items missing")?
                {
                    result.push(json!({"name":d["name"],"category":category,"status":match d["state"].as_i64(){Some(1)=>"idle",Some(2)=>"working",Some(3)=>"error",_=>"unknown"},"raw":d}));
                }
            }
            Ok(json!(result))
        }
        "washer-xiaolan-buildings" => {
            let v = response(c.get(format!("{XIAOLAN}/buildings/list")).send().await?).await?;
            parse_xiaolan(&v, None)
        }
        "washer-xiaolan-floors" => {
            let b = arg(a, "building")?;
            let v = response(
                c.get(format!("{XIAOLAN}/buildings/{}", urlencoding::encode(b)))
                    .send()
                    .await?,
            )
            .await?;
            parse_xiaolan(&v, Some(b))
        }
        "calendar-export" => {
            let events: Value = serde_json::from_slice(&std::fs::read(arg(a, "input")?)?)?;
            let data = export_ics(&events)?;
            write_new(arg(a, "output")?, data.as_bytes())?;
            Ok(json!({"output":arg(a,"output")?,"events":events.as_array().unwrap().len()}))
        }
        "schedule-sync-send" | "schedule-sync-receive" => {
            sync(c, op == "schedule-sync-send", a).await
        }
        _ => bail!("Unknown app operation"),
    }
}

async fn sync(c: &Client, sending: bool, a: &Value) -> Result<Value> {
    sync_at(c, ROOT, sending, a, |peer| {
        eprintln!(
            "Peer pairing token: {}. Verify it on the other device. Type yes to transfer:",
            serde_json::to_string(peer)?
        );
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        Ok(answer.trim() == "yes")
    })
    .await
}
async fn invoke(c: &Client, url: &str, target: &str, args: Value) -> Result<()> {
    let body = format!(
        "{}\u{1e}",
        json!({"type":1,"target":target,"arguments":args})
    );
    c.post(url)
        .header(reqwest::header::CONTENT_TYPE, "text/plain;charset=UTF-8")
        .body(body)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}
fn frames(body: &str) -> Result<Vec<Value>> {
    if !body.is_empty() && !body.ends_with('\u{1e}') {
        bail!("Incomplete SignalR record")
    }
    body.split('\u{1e}')
        .filter(|s| !s.trim().is_empty())
        .map(|s| serde_json::from_str(s).context("Invalid SignalR JSON frame"))
        .collect()
}
async fn sync_at<F>(
    c: &Client,
    root: &str,
    sending: bool,
    a: &Value,
    mut confirm: F,
) -> Result<Value>
where
    F: FnMut(&str) -> Result<bool>,
{
    let id = arg(a, "match_id")?;
    let payload = if sending {
        Some(std::fs::read_to_string(arg(a, "input")?)?)
    } else {
        None
    };
    if let Some(p) = &payload {
        let _: Value = serde_json::from_str(p).context("Schedule input must be valid JSON")?;
    }
    if !sending && std::path::Path::new(arg(a, "output")?).exists() {
        bail!("Receive output already exists; choose a new file")
    }
    let negotiate = response(
        c.post(format!(
            "{root}/schedulesynchub/negotiate?negotiateVersion=1"
        ))
        .send()
        .await?,
    )
    .await?;
    if negotiate.get("error").is_some() {
        bail!("SignalR negotiation rejected")
    }
    if let Some(transports) = negotiate["availableTransports"].as_array() {
        if !transports.iter().any(|t| t["transport"] == "LongPolling") {
            bail!("SignalR server does not advertise LongPolling")
        }
    }
    let token = negotiate["connectionToken"]
        .as_str()
        .context("SignalR negotiation did not return connectionToken")?;
    let url = format!("{root}/schedulesynchub?id={}", urlencoding::encode(token));
    let deadline = Instant::now()
        + Duration::from_secs(a["timeout_seconds"].as_u64().unwrap_or(180).clamp(10, 3600));
    let result = async {
        // ASP.NET Core long-poll transport requires an initial empty poll before sending handshake.
        let first = c.get(&url).timeout(Duration::from_secs(30)).send().await?;
        if first.status() == reqwest::StatusCode::NO_CONTENT {
            bail!("SignalR connection closed during initialization")
        }
        first.error_for_status()?;
        c.post(&url)
            .header(reqwest::header::CONTENT_TYPE, "text/plain;charset=UTF-8")
            .body("{\"protocol\":\"json\",\"version\":1}\u{1e}")
            .send()
            .await?
            .error_for_status()?;
        let mut handshake_complete = false;
        let mut matched = false;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("Schedule sync timed out waiting for peer")
            }
            // SignalR long polls commonly remain open for 90 seconds. A 40-second hard
            // failure disconnects valid idle sessions. Transport timeouts are safe to repoll.
            let r = match c
                .get(&url)
                .timeout(remaining.min(Duration::from_secs(100)))
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) if e.is_timeout() => continue,
                Err(e) => return Err(e.into()),
            };
            if r.status() == reqwest::StatusCode::NO_CONTENT {
                bail!("Schedule sync closed before completion")
            }
            let text = r.error_for_status()?.text().await?;
            for v in frames(&text)? {
                if !handshake_complete {
                    if v.get("error").is_some() {
                        bail!("SignalR JSON handshake rejected")
                    }
                    if v.get("type").is_some() {
                        bail!("SignalR sent hub message before handshake acknowledgement")
                    }
                    handshake_complete = true;
                    invoke(c, &url, "StartMatch", json!([id, sending])).await?;
                    continue;
                }
                if v["type"] == 7 {
                    bail!("SignalR connection closed")
                }
                if v["type"] == 3 && v.get("error").is_some() {
                    bail!("SignalR invocation rejected")
                }
                if v["type"] != 1 {
                    continue;
                }
                match v["target"].as_str() {
                    Some("ConfirmMatch") => {
                        let peer = v["arguments"][0]
                            .as_str()
                            .context("Missing pairing token")?;
                        if !confirm(peer)? {
                            bail!("Pairing cancelled")
                        }
                        invoke(c, &url, "ConfirmMatch", json!([id, peer, sending])).await?;
                        matched = true;
                    }
                    Some("SetTarget") if sending => {
                        if !matched {
                            bail!("Server selected a peer before pairing was confirmed")
                        }
                        let target = v["arguments"][0]
                            .as_str()
                            .context("Missing target identifier")?;
                        invoke(
                            c,
                            &url,
                            "SendToTarget",
                            json!([target, payload.as_ref().unwrap()]),
                        )
                        .await?;
                        return Ok(json!({"sent_to_server":true,"peer_delivery_confirmed":false}));
                    }
                    Some("ReceiveSchedules") if !sending => {
                        if !matched {
                            bail!("Received schedules before pairing was confirmed")
                        }
                        let data = v["arguments"][0]
                            .as_str()
                            .context("Missing schedule data")?;
                        let _: Value = serde_json::from_str(data)
                            .context("Received schedule is not valid JSON")?;
                        write_new(arg(a, "output")?, data.as_bytes())?;
                        return Ok(json!({"received":true,"output":arg(a,"output")?}));
                    }
                    _ => {}
                }
            }
        }
    }
    .await;
    // Close the server connection on success, rejected pairing, malformed frames and errors.
    let _ = c.delete(&url).send().await;
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xiaolan_status() {
        assert_eq!(
            xs(&json!({"deviceState":{"isOnline":1,"runState":7,"fault":0}})),
            "idle"
        );
        assert_eq!(xs(&json!({"deviceState":{"isOnline":0}})), "offline");
    }
    #[test]
    fn calendar_timezone_and_escape() {
        let s=export_ics(&json!([{"title":"A,B\nC","start":"2026-11-12T10:30:00+08:00","end":"2026-11-12T12:00:00+08:00"}])).unwrap();
        assert!(s.contains("DTSTART:20261112T023000Z"));
        assert!(s.contains("SUMMARY:A\\,B\\nC"));
        assert!(s.ends_with("END:VCALENDAR\r\n"));
    }
    #[test]
    fn calendar_folds_utf8_without_splitting_codepoints() {
        let line = format!("SUMMARY:{}", "清华大学🦀".repeat(30));
        let folded = folded(&line);
        assert!(folded.split("\r\n").all(|physical| physical.len() <= 75));
        assert_eq!(folded.trim_end_matches("\r\n").replace("\r\n ", ""), line);
        assert!(!folded.contains('\u{fffd}'));
    }
    #[test]
    fn frame_parser_never_accepts_unterminated_or_invalid_records() {
        assert_eq!(frames("{}\u{1e}").unwrap(), vec![json!({})]);
        assert!(frames("{}").is_err());
        assert!(frames("bad\u{1e}").is_err());
    }
    #[tokio::test]
    async fn signalr_mock_send_handshake_pairing_and_cleanup() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        use wiremock::{
            matchers::{body_string, method, path, query_param},
            Mock, MockServer, ResponseTemplate,
        };
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/schedulesynchub/negotiate")).and(query_param("negotiateVersion","1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"connectionToken":"test-token","availableTransports":[{"transport":"LongPolling"}]}))).expect(1).mount(&server).await;
        let count = Arc::new(AtomicUsize::new(0));
        let counter = count.clone();
        Mock::given(method("GET"))
            .and(path("/schedulesynchub"))
            .and(query_param("id", "test-token"))
            .respond_with(move |_: &wiremock::Request| {
                let body = match counter.fetch_add(1, Ordering::SeqCst) {
                    0 => String::new(),
                    1 => "{}\u{1e}".into(),
                    2 => format!(
                        "{}\u{1e}",
                        json!({"type":1,"target":"ConfirmMatch","arguments":["peer-token"]})
                    ),
                    3 => format!(
                        "{}\u{1e}",
                        json!({"type":1,"target":"SetTarget","arguments":["peer-target"]})
                    ),
                    _ => format!("{}\u{1e}", json!({"type":7})),
                };
                ResponseTemplate::new(200).set_body_string(body)
            })
            .expect(4)
            .mount(&server)
            .await;
        let body = format!("{}\u{1e}", json!({"protocol":"json","version":1}));
        // serde_json map order is irrelevant to the server, but the implementation uses
        // the standard fixed handshake ordering below.
        let _ = body;
        Mock::given(method("POST"))
            .and(path("/schedulesynchub"))
            .and(body_string("{\"protocol\":\"json\",\"version\":1}\u{1e}"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        for (target, args) in [
            ("StartMatch", json!(["match", true])),
            ("ConfirmMatch", json!(["match", "peer-token", true])),
            ("SendToTarget", json!(["peer-target", "[]"])),
        ] {
            Mock::given(method("POST"))
                .and(path("/schedulesynchub"))
                .and(body_string(format!(
                    "{}\u{1e}",
                    json!({"type":1,"target":target,"arguments":args})
                )))
                .respond_with(ResponseTemplate::new(200))
                .expect(1)
                .mount(&server)
                .await;
        }
        Mock::given(method("DELETE"))
            .and(path("/schedulesynchub"))
            .respond_with(ResponseTemplate::new(202))
            .expect(1)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("schedule.json");
        std::fs::write(&input, "[]").unwrap();
        let result = sync_at(
            &Client::new(),
            &server.uri(),
            true,
            &json!({"match_id":"match","input":input}),
            |token| {
                assert_eq!(token, "peer-token");
                Ok(true)
            },
        )
        .await
        .unwrap();
        assert_eq!(result["sent_to_server"], true);
        assert_eq!(result["peer_delivery_confirmed"], false);
        server.verify().await;
    }
    #[tokio::test]
    async fn signalr_rejects_handshake_error_and_closes() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/schedulesynchub/negotiate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"connectionToken":"t"})))
            .mount(&server)
            .await;
        let count = Arc::new(AtomicUsize::new(0));
        Mock::given(method("GET"))
            .and(path("/schedulesynchub"))
            .respond_with(move |_: &wiremock::Request| {
                ResponseTemplate::new(200).set_body_string(
                    if count.fetch_add(1, Ordering::SeqCst) == 0 {
                        ""
                    } else {
                        "{\"error\":\"rejected\"}\u{1e}"
                    },
                )
            })
            .expect(2)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/schedulesynchub"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/schedulesynchub"))
            .respond_with(ResponseTemplate::new(202))
            .expect(1)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("new.json");
        let err = sync_at(
            &Client::new(),
            &server.uri(),
            false,
            &json!({"match_id":"m","output":output}),
            |_| Ok(true),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("handshake rejected"));
        assert!(!output.exists());
        server.verify().await;
    }
    #[test]
    fn xiaolan_validates_ids_and_fallback_labels() {
        let v = json!({ORG:{"buildings":{"b":{"facilities":[{"store":{"floor":"2"},"storeDetail":{"storeId":"s"},"devices":[{"deviceId":"d","deviceState":{"isOnline":"1","runState":"5"}}]}]}}}});
        let result = parse_xiaolan(&v, Some("b")).unwrap();
        assert_eq!(result["floors"][0]["id"], "s");
        assert_eq!(result["floors"][0]["washers"][0]["name"], "d");
        assert_eq!(result["floors"][0]["washers"][0]["status"], "working");
        let bad = json!({ORG:{"buildings":{"b":{"facilities":[{"store":{"storeId":"s"}}]}}}});
        assert!(parse_xiaolan(&bad, Some("b")).is_err());
    }
    #[test]
    fn jieli_status_and_location_mapping() {
        let v = json!({"data":[{"floorName":"2F","macUnionCode":"洗衣机 A01","status":"工作中 剩余20分钟"}]});
        let parsed = jieli_floors(&v, &json!({"A01":"北侧"})).unwrap();
        assert_eq!(parsed[0]["washers"][0]["eta"], 20);
        assert_eq!(parsed[0]["washers"][0]["status"], "working");
        assert_eq!(parsed[0]["washers"][0]["location"], "北侧");
    }
}
