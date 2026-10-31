//! Native adapters ported from thu-info-lib library.ts and sports.ts (upstream BSL; see bundled license).
//! Requires service cookies; no browser scripts or JavaScript runtime are executed.
use anyhow::{anyhow, bail, Context, Result};
use chrono::{FixedOffset, Utc};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const SEAT: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421e3f24088693c6152301c9aa596522b204c02212b859d0a19";
const ROOM: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f3f643d22b396a1e6a1b80a29f5d363409e413829737d1";
const SPORT: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421a5a70f8834396657761d88e29d51367b6a00";
const SOCKET: &str = "https://app.cs.tsinghua.edu.cn/api/socket";
fn req(a: &Value, k: &str) -> Result<String> {
    a.get(k)
        .filter(|v| !v.is_null())
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| v.to_string())
        })
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("missing parameter: {k}"))
}
fn opt(a: &Value, k: &str, d: &str) -> String {
    req(a, k).unwrap_or_else(|_| d.into())
}
fn date(a: &Value) -> String {
    opt(
        a,
        "date",
        &Utc::now()
            .with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap())
            .format("%Y-%m-%d")
            .to_string(),
    )
}
fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("static selector")
}
fn text(e: scraper::ElementRef<'_>) -> String {
    e.text().collect::<String>().trim().to_owned()
}
fn number(s: &str) -> Value {
    s.parse::<f64>().ok().map_or(Value::Null, |v| json!(v))
}
async fn body(r: reqwest::RequestBuilder) -> Result<String> {
    let r = r.send().await?.error_for_status()?;
    let charset = r
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    let bytes = r.bytes().await?;
    let decoded = if charset.contains("gb") {
        encoding_rs::GBK.decode(&bytes).0.into_owned()
    } else {
        String::from_utf8(bytes.to_vec())
            .unwrap_or_else(|_| encoding_rs::GBK.decode(&bytes).0.into_owned())
    };
    if decoded.contains("/do/off/ui/auth/login/form/")
        || decoded.contains("用户登陆超时")
        || decoded.contains("找回密码")
    {
        bail!("service session expired or not authenticated; establish the service session first");
    }
    Ok(decoded)
}
async fn get(c: &Client, url: &str) -> Result<String> {
    body(c.get(url)).await
}
async fn get_json(c: &Client, url: &str) -> Result<Value> {
    serde_json::from_str(&get(c, url).await?).context("expected JSON; service may require login")
}
async fn form(c: &Client, url: &str, p: &Value) -> Result<String> {
    let m = p
        .as_object()
        .ok_or_else(|| anyhow!("form must be an object"))?
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                v.as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| v.to_string()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    body(c.post(url).form(&m)).await
}
async fn seat_list(c: &Client, path: &str) -> Result<Value> {
    let v = get_json(c, &format!("{SEAT}{path}")).await?;
    v.pointer("/data/list").cloned().ok_or_else(|| {
        anyhow!("library response has no data.list; check session and response schema")
    })
}
async fn cab(c: &Client, path: &str, p: Option<Value>) -> Result<Value> {
    let url = format!("{ROOM}{path}");
    let s = match p {
        Some(p) => body(c.post(url).json(&p)).await?,
        None => get(c, &url).await?,
    };
    let v: Value = serde_json::from_str(&s)?;
    if v["code"] != 0 {
        bail!(
            "library room API: {}",
            v["message"].as_str().unwrap_or("unknown error")
        );
    }
    Ok(v["data"].clone())
}
async fn token(c: &Client) -> Result<String> {
    let h = get(c, &format!("{SEAT}/home/web/f_second")).await?;
    parse_token(&h)
}
fn parse_token(h: &str) -> Result<String> {
    Regex::new(r#"access_token\s*[:=]\s*["']([^"']+)["']"#)?
        .captures(h)
        .map(|c| c[1].to_owned())
        .ok_or_else(|| anyhow!("library access token absent; sign in to seat service first"))
}
async fn day(c: &Client, a: &Value) -> Result<Value> {
    let id = req(a, "section_id")?;
    let days = seat_list(c, &format!("/api.php/areadays/{id}")).await?;
    let d = date(a);
    days.as_array()
        .and_then(|v| v.iter().find(|v| v["day"].as_str() == Some(&d)))
        .cloned()
        .ok_or_else(|| anyhow!("no booking segment for {d}"))
}
fn segment_time(d: &Value, k: &str) -> Result<String> {
    let s = d[k]["date"]
        .as_str()
        .or_else(|| d[k].as_str())
        .ok_or_else(|| anyhow!("missing {k}"))?;
    Ok(if s.len() >= 16 {
        s[11..16].into()
    } else {
        s.into()
    })
}
fn booking_response(v: Value) -> Result<Value> {
    if v.get("status").is_some_and(|s| s == false || s == 0) {
        bail!(
            "booking rejected: {}",
            v.get("msg")
                .or_else(|| v.get("message"))
                .unwrap_or(&Value::Null)
        );
    }
    Ok(v)
}

type OperationDefinition = (
    &'static str,
    bool,
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
);

pub fn operations() -> Value {
    let defs: &[OperationDefinition] = &[
        (
            "library.list",
            false,
            "List seat-booking libraries",
            &[],
            &[],
        ),
        (
            "library.floors",
            false,
            "List floors for a library",
            &["library_id"],
            &[],
        ),
        (
            "library.sections",
            false,
            "List floor sections and availability",
            &["floor_id"],
            &["date"],
        ),
        (
            "library.days",
            false,
            "Available reservation dates and segments",
            &["section_id"],
            &[],
        ),
        (
            "library.seats",
            false,
            "Seat availability for a section/date",
            &["section_id"],
            &["date", "start_time", "end_time"],
        ),
        (
            "library.book",
            true,
            "Book one seat using a fetched access token",
            &["seat_id", "section_id", "user_id", "type"],
            &["date"],
        ),
        (
            "library.records",
            false,
            "List own seat reservations",
            &[],
            &[],
        ),
        (
            "library.cancel",
            true,
            "Cancel a seat reservation",
            &["id", "user_id"],
            &[],
        ),
        (
            "library.socket-status",
            false,
            "Crowdsourced seat socket status",
            &["section_id"],
            &[],
        ),
        (
            "library.socket-update",
            true,
            "Update crowdsourced socket availability",
            &["seat_id", "available"],
            &[],
        ),
        (
            "library.room-user",
            false,
            "Current CAB room-booking account",
            &[],
            &[],
        ),
        (
            "library.room-list",
            false,
            "Room kinds and devices",
            &[],
            &[],
        ),
        (
            "library.room-resources",
            false,
            "Room availability and rules",
            &["date", "kind_id"],
            &[],
        ),
        (
            "library.room-members",
            false,
            "Find member account IDs",
            &["keyword"],
            &[],
        ),
        (
            "library.room-book",
            true,
            "Book room with explicit member account IDs",
            &["device_id", "start", "end", "members"],
            &[],
        ),
        (
            "library.room-records",
            false,
            "Own room bookings in date range",
            &["begin", "end"],
            &[],
        ),
        (
            "library.room-cancel",
            true,
            "Cancel a room booking",
            &["uuid"],
            &[],
        ),
        (
            "library.room-email",
            true,
            "Update room booking email",
            &["email"],
            &[],
        ),
        (
            "sports.venues",
            false,
            "Known venue and item IDs from upstream",
            &[],
            &[],
        ),
        (
            "sports.resources",
            false,
            "Get resource hashes, costs and reservation limits",
            &["gym_id", "item_id", "date"],
            &[],
        ),
        ("sports.phone", false, "Current reservation phone", &[], &[]),
        (
            "sports.phone-update",
            true,
            "Set reservation contact phone",
            &["phone", "user_id"],
            &[],
        ),
        (
            "sports.captcha",
            false,
            "Captcha image URL to download with same session",
            &[],
            &[],
        ),
        (
            "sports.records",
            false,
            "Paid and unpaid sports reservations",
            &[],
            &[],
        ),
        (
            "sports.book",
            true,
            "Create a sports reservation; never auto-pay",
            &[
                "total_cost",
                "phone",
                "gym_id",
                "item_id",
                "date",
                "captcha",
                "resource_hash",
            ],
            &[],
        ),
        (
            "sports.cancel",
            true,
            "Cancel a sports booking",
            &["book_id"],
            &[],
        ),
        (
            "sports.payment-start",
            true,
            "Prepare payment form for manual continuation; does not pay",
            &["pay_id"],
            &["receipt_title"],
        ),
        (
            "sports.payment-qr",
            true,
            "Create Alipay QR payment handoff for an existing reservation; does not pay",
            &["pay_id"],
            &["receipt_title"],
        ),
    ];
    Value::Array(defs.iter().map(|(n,m,d,r,o)|json!({"name":n,"mutating":m,"description":d,"required":r,"optional":o,"upstream":if n.starts_with("library."){"thu-info-lib/src/lib/library.ts"}else{"thu-info-lib/src/lib/sports.ts"}})).collect())
}

pub async fn execute(c: &Client, operation: &str, a: &Value) -> Result<Value> {
    match operation {
        "library.list" => seat_list(c, "/api.php/areas/1/tree/1").await,
        "library.floors" => {
            seat_list(c, &format!("/api.php/areas/{}", req(a, "library_id")?)).await
        }
        "library.sections" => {
            seat_list(
                c,
                &format!("/api.php/areas/{}/date/{}", req(a, "floor_id")?, date(a)),
            )
            .await
        }
        "library.days" => {
            seat_list(c, &format!("/api.php/areadays/{}", req(a, "section_id")?)).await
        }
        "library.seats" => {
            let d = day(c, a).await?;
            let mut start = segment_time(&d, "startTime")?;
            let date = date(a);
            let now = Utc::now().with_timezone(&FixedOffset::east_opt(28800).unwrap());
            if date == now.format("%Y-%m-%d").to_string() {
                start = start.max(now.format("%H:%M").to_string());
            }
            let end = segment_time(&d, "endTime")?;
            let p = [
                ("area", req(a, "section_id")?),
                ("segment", req(&d, "id")?),
                ("day", date),
                ("startTime", opt(a, "start_time", &start)),
                ("endTime", opt(a, "end_time", &end)),
            ];
            let v: Value = serde_json::from_str(
                &body(c.get(format!("{SEAT}/api.php/spaces_old")).query(&p)).await?,
            )?;
            Ok(v.pointer("/data/list")
                .cloned()
                .ok_or_else(|| anyhow!("missing seat data.list"))?)
        }
        "library.book" => {
            let d = day(c, a).await?;
            let p = json!({"access_token":token(c).await?,"userid":req(a,"user_id")?,"segment":d["id"],"type":req(a,"type")?,"operateChannel":2});
            booking_response(serde_json::from_str(
                &form(
                    c,
                    &format!("{SEAT}/api.php/spaces/{}/book", req(a, "seat_id")?),
                    &p,
                )
                .await?,
            )?)
        }
        "library.records" => {
            token(c).await?;
            let h = get(c, &format!("{SEAT}/user/index/book")).await?;
            parse_seat_records(&h)
        }
        "library.cancel" => {
            let id = req(a, "id")?;
            booking_response(serde_json::from_str(&form(c,&format!("{SEAT}/api.php/profile/books/{id}"),&json!({"_method":"delete","id":id,"userid":req(a,"user_id")?,"access_token":token(c).await?,"operateChannel":2})).await?)?)
        }
        "library.socket-status" => {
            let s = body(c.get(SOCKET).query(&[("sectionid", req(a, "section_id")?)])).await?;
            Ok(serde_json::from_str(&s)?)
        }
        "library.socket-update" => {
            let available = a
                .get("available")
                .and_then(Value::as_bool)
                .ok_or_else(|| anyhow!("available must be boolean"))?;
            let s = body(c.post(SOCKET).json(
                &json!({"seatId":req(a,"seat_id")?.parse::<u64>()?,"isavailable":available}),
            ))
            .await?;
            Ok(serde_json::from_str(&s).unwrap_or(json!({"response":s})))
        }
        "library.room-user" => cab(c, "/ic-web/auth/userInfo", None).await,
        "library.room-list" => cab(c, "/ic-web/roomDevice/roomInfos", None).await,
        "library.room-resources" => {
            let date = req(a, "date")?.replace('-', "");
            let kind = req(a, "kind_id")?.parse::<u64>()?;
            cab(
                c,
                &format!("/ic-web/reserve?sysKind=1&resvDates={date}&kindIds={kind}"),
                None,
            )
            .await
        }
        "library.room-members" => {
            let mut url = reqwest::Url::parse(&format!("{ROOM}/ic-web/account/getMembers"))?;
            url.query_pairs_mut()
                .append_pair("page", "1")
                .append_pair("pageNum", "10")
                .append_pair("key", &req(a, "keyword")?);
            cab(c, &url.as_str()[ROOM.len()..], None).await
        }
        "library.room-book" => {
            let user = cab(c, "/ic-web/auth/userInfo", None).await?;
            let members = a["members"]
                .as_array()
                .ok_or_else(|| anyhow!("members must be an array of numeric CAB accNo values"))?;
            if members.is_empty() || members.iter().any(|x| !x.is_u64()) {
                bail!("members must contain numeric CAB accNo values, not student IDs");
            }
            let start = req(a, "start")?;
            let end = req(a, "end")?;
            validate_room_times(&start, &end)?;
            cab(c,"/ic-web/reserve",Some(json!({"sysKind":1,"appAccNo":user["accNo"],"memberKind":1,"resvBeginTime":start,"resvEndTime":end,"testName":"","resvKind":2,"resvProperty":0,"appUrl":"","resvMember":members,"resvDev":[req(a,"device_id")?.parse::<u64>()?],"memo":"","captcha":"","addServices":[]}))).await
        }
        "library.room-records" => {
            let mut u=reqwest::Url::parse(&format!("{ROOM}/ic-web/reserve/resvInfo?needStatus=8454&orderKey=gmt_create&orderModel=desc"))?;
            u.query_pairs_mut()
                .append_pair("beginDate", &req(a, "begin")?)
                .append_pair("endDate", &req(a, "end")?);
            cab(c, &u.as_str()[ROOM.len()..], None).await
        }
        "library.room-cancel" => {
            cab(
                c,
                "/ic-web/reserve/delete",
                Some(json!({"uuid":req(a,"uuid")?})),
            )
            .await
        }
        "library.room-email" => {
            cab(
                c,
                "/ic-web/account/update",
                Some(json!({"email":req(a,"email")?})),
            )
            .await
        }
        "sports.venues" => Ok(
            json!([{"name":"气膜馆羽毛球场","gym_id":"3998000","item_id":"4045681"},{"name":"气膜馆乒乓球场","gym_id":"3998000","item_id":"4037036"},{"name":"综体篮球场","gym_id":"4797914","item_id":"4797898"},{"name":"综体羽毛球场","gym_id":"4797914","item_id":"4797899"},{"name":"西体羽毛球场","gym_id":"4836273","item_id":"4836196"},{"name":"西体台球","gym_id":"4836273","item_id":"14567218"},{"name":"紫荆网球场","gym_id":"5843934","item_id":"5845263"},{"name":"西网球场","gym_id":"5843934","item_id":"10120539"}]),
        ),
        "sports.resources" => {
            let q = [
                ("gymnasium_id", req(a, "gym_id")?),
                ("item_id", req(a, "item_id")?),
                ("time_date", req(a, "date")?),
            ];
            let base = body(
                c.get(format!(
                    "{SPORT}/gymbook/gymBookAction.do?ms=viewGymBook&viewType=m"
                ))
                .query(&q),
            )
            .await?;
            let detail = body(
                c.get(format!(
                    "{SPORT}/gymsite/cacheAction.do?ms=viewBook&userType=1"
                ))
                .query(&q),
            )
            .await?;
            let count = capture(&base, r"var limitBookCount = '(\d+?)';")?;
            let init = capture(&base, r"var limitBookInit = '(\d+?)';")?;
            Ok(
                json!({"count":count.parse::<u64>()?,"init":init.parse::<u64>()?,"phone":get(c,&format!("{SPORT}/gymbook/gymBookAction.do?ms=hadContactOrNot")).await?,"data":parse_sports_resources(&detail)?}),
            )
        }
        "sports.phone" => Ok(
            json!({"phone":get(c,&format!("{SPORT}/gymbook/gymBookAction.do?ms=hadContactOrNot")).await?}),
        ),
        "sports.phone-update" => {
            let phone = req(a, "phone")?;
            if !Regex::new(r"^1[3-9][0-9]{9}$")?.is_match(&phone) {
                bail!("invalid mainland mobile number");
            }
            let s = body(
                c.post(format!(
                    "{SPORT}/gymbook/gymBookAction.do?ms=doUpdateContactInformation"
                ))
                .query(&[("cell_phone", phone), ("gzzh", req(a, "user_id")?)]),
            )
            .await?;
            Ok(json!({"response":s}))
        }
        "sports.captcha" => Ok(
            json!({"url":format!("{SPORT}/Kaptcha.jpg?{}=",Utc::now().timestamp_millis()),"instruction":"Download with the same cookie session and supply the human-entered captcha in sports.book."}),
        ),
        "sports.records" => {
            let unpaid = get(c, &format!("{SPORT}/pay/payAction.do?ms=getOrdersForNopay")).await?;
            let paid = get(c, &format!("{SPORT}/pay/payAction.do?ms=getOrdersForUnpay")).await?;
            Ok(
                json!({"unpaid":parse_sports_records(&unpaid,false)?,"paid":parse_sports_records(&paid,true)?}),
            )
        }
        "sports.book" => {
            let cost = req(a, "total_cost")?.parse::<f64>()?;
            if !cost.is_finite() || cost < 0.0 {
                bail!("total_cost must be a non-negative number");
            }
            let p = json!({"bookData.totalCost":cost,"bookData.book_person_zjh":"","bookData.book_person_name":"","bookData.book_person_phone":req(a,"phone")?,"bookData.book_mode":"from-phone","gymnasium_idForCache":req(a,"gym_id")?,"item_idForCache":req(a,"item_id")?,"time_dateForCache":req(a,"date")?,"userTypeNumForCache":1,"putongRes":"putongRes","code":req(a,"captcha")?,"selectedPayWay":1,"allFieldTime":format!("{}#{}",req(a,"resource_hash")?,req(a,"date")?)});
            let v:Value=serde_json::from_str(&form(c,&format!("{SPORT}/gymbook/gymbook/gymBookAction.do?vpn-12-o1-50.tsinghua.edu.cn=&ms=saveGymBook"),&p).await?)?;
            if v["msg"] != "预定成功" {
                bail!("sports reservation rejected: {}", v["msg"]);
            }
            Ok(
                json!({"result":v,"payment_completed":false,"next":"Use sports.records to obtain pay_id; payment requires a separate explicit action."}),
            )
        }
        "sports.cancel" => Ok(
            json!({"response":form(c,&format!("{SPORT}/gymbook/gymBookAction.do?ms=unsubscribe"),&json!({"bookId":req(a,"book_id")?})).await?}),
        ),
        "sports.payment-start" | "sports.payment-qr" => {
            let title = opt(a, "receipt_title", "清华大学");
            if !["清华大学", "清华大学工会", "清华大学教育基金会"].contains(&title.as_str())
            {
                bail!("unsupported receipt title");
            }
            let p = [("book_ids".into(), req(a, "pay_id")?), ("xm".into(), title)]
                .into_iter()
                .collect();
            let s = body(
                c.post(format!("{SPORT}/pay/payAction.do?ms=newPayForLater"))
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(gbk_form(&p)?),
            )
            .await?;
            if operation == "sports.payment-qr" {
                return payment_qr(c, &s).await;
            }
            let form = parse_payment_form(&s)?;
            Ok(
                json!({"payment_completed":false,"manual_handoff":form,"warning":"This is a prepared payment form, not a completed payment or Alipay QR code. Continue in the official service."}),
            )
        }
        _ => bail!("unknown reservation operation: {operation}"),
    }
}
fn capture(h: &str, p: &str) -> Result<String> {
    Regex::new(p)?
        .captures(h)
        .map(|c| c[1].into())
        .ok_or_else(|| anyhow!("service response did not match expected schema"))
}
fn gbk_form(p: &BTreeMap<String, String>) -> Result<String> {
    let mut parts = Vec::new();
    for (k, v) in p {
        let (b, _, bad) = encoding_rs::GBK.encode(v);
        if bad {
            bail!("value not representable in GBK");
        }
        let enc = |b: &[u8]| {
            b.iter()
                .map(|x| {
                    if x.is_ascii_alphanumeric() || b"-._~".contains(x) {
                        (*x as char).to_string()
                    } else {
                        format!("%{x:02X}")
                    }
                })
                .collect::<String>()
        };
        parts.push(format!("{}={}", enc(k.as_bytes()), enc(&b)));
    }
    Ok(parts.join("&"))
}
fn parse_payment_form(h: &str) -> Result<Value> {
    let d = Html::parse_document(h);
    let f = d
        .select(&sel("form"))
        .next()
        .ok_or_else(|| anyhow!("payment response contains no form"))?;
    let action = f
        .value()
        .attr("action")
        .ok_or_else(|| anyhow!("payment form missing action"))?;
    let fields = f
        .select(&sel("input[name]"))
        .map(|i| {
            (
                i.value().attr("name").unwrap().to_owned(),
                json!(i.value().attr("value").unwrap_or("")),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    Ok(json!({"action":action,"method":f.value().attr("method").unwrap_or("get"),"fields":fields}))
}
fn parse_seat_records(h: &str) -> Result<Value> {
    let d = Html::parse_document(h);
    if d.select(&sel("tbody")).next().is_none() {
        bail!("seat booking response has no table; check login");
    }
    let r = Regex::new(r#"menuDel\(['"]?([^'"),]+)"#)?;
    let mut rows = Vec::new();
    for row in d.select(&sel("tbody tr")) {
        let td = row.select(&sel("td")).map(text).collect::<Vec<_>>();
        if td.len() < 6 {
            continue;
        }
        let id = row
            .select(&sel("[onclick]"))
            .filter_map(|e| e.value().attr("onclick"))
            .find_map(|s| r.captures(s).map(|c| c[1].to_owned()));
        rows.push(json!({"id":td.get(1),"pos":td.get(2),"time":td.get(3),"status":td.get(5),"delId":id,"cells":td}));
    }
    Ok(json!(rows))
}
fn parse_sports_resources(h: &str) -> Result<Value> {
    let mut result = BTreeMap::<String, Value>::new();
    let r = Regex::new(
        r"resourceArray\.push\(\{id:'(.*?)',time_session:'(.*?)',field_name:'(.*?)',overlaySize:'(.*?)',can_net_book:'(.*?)'\}\);[\s\S]+?resourcesm\.put\('(.*?)', '(.*?)'\)",
    )?;
    for c in r.captures_iter(h) {
        if c[1] == c[6] {
            result.insert(c[1].into(),json!({"resId":&c[1],"resHash":&c[7],"timeSession":&c[2],"fieldName":&c[3],"overlaySize":number(&c[4]),"canNetBook":&c[5]=="1"}));
        }
    }
    for c in Regex::new(r"addCost\('(.*?)','(.*?)'\);")?.captures_iter(h) {
        if let Some(v) = result.get_mut(&c[1]) {
            v["cost"] = number(&c[2]);
        }
    }
    for c in Regex::new(r"markResStatus\('(.*?)','(.*?)','(.*?)'\);")?.captures_iter(h) {
        if let Some(v) = result.get_mut(&c[2]) {
            v["bookId"] = json!(&c[1]);
            v["locked"] = json!(&c[3] == "1");
        }
    }
    for c in Regex::new(r"markStatusColor\('(.*?)','(.*?)','(.*?)','(.*?)'\);")?.captures_iter(h) {
        if let Some(v) = result.get_mut(&c[1]) {
            v["userType"] = json!(&c[2]);
            v["paymentStatus"] = json!(&c[3] == "1");
        }
    }
    if result.is_empty() && !h.contains("resourceArray") {
        bail!("sports resource page schema missing; check session");
    }
    Ok(json!(result.into_values().collect::<Vec<_>>()))
}
fn parse_sports_records(h: &str, paid: bool) -> Result<Value> {
    let d = Html::parse_document(h);
    if d.select(&sel("table")).next().is_none() {
        bail!("sports reservation page missing table");
    }
    let row_selector = sel(if paid {
        "tr[style='display:none']"
    } else {
        "tbody tr"
    });
    let rows = d.select(&row_selector);
    let pay_re = Regex::new(r"payNow\('([^']+)'")?;
    let cancel_re = Regex::new(r"unsubscribe(?:Online)?\('([^']+)'")?;
    let mut out = Vec::new();
    for row in rows {
        let root = if paid {
            row.select(&sel("tbody tr")).next().unwrap_or(row)
        } else {
            row
        };
        let cells = root.select(&sel("td")).map(text).collect::<Vec<_>>();
        let offset = if paid { 2 } else { 0 };
        if cells.len() < offset + 4 {
            continue;
        }
        let html = root.html();
        let pay = pay_re.captures(&html).map(|c| c[1].to_owned());
        let book = cancel_re.captures(&html).map(|c| c[1].to_owned());
        out.push(json!({"name":cells.get(offset),"field":cells.get(offset+1),"time":cells.get(offset+2),"price":cells.get(offset+3),"method":if paid{Some("已支付")}else{cells.get(4).map(String::as_str)},"payId":pay,"bookId":book,"bookTimestamp":root.select(&sel("span[time]")).next().and_then(|e|e.value().attr("time")).and_then(|s|s.parse::<i64>().ok())}));
    }
    Ok(json!(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_parser() {
        assert_eq!(
            parse_token("var access_token = \"abc123\";").unwrap(),
            "abc123"
        );
        assert!(parse_token("login page").is_err());
    }
    #[test]
    fn resource_parser() {
        let html="resourceArray.push({id:'a',time_session:'10:00',field_name:'one',overlaySize:'1',can_net_book:'1'});\nresourcesm.put('a', 'hash');addCost('a','20');markResStatus('book','a','1');markStatusColor('a','1','0','');";
        let v = parse_sports_resources(html).unwrap();
        assert_eq!(v[0]["resHash"], "hash");
        assert_eq!(v[0]["cost"], 20.0);
        assert_eq!(v[0]["locked"], true);
    }
    #[test]
    fn payment_form_is_only_data() {
        let v = parse_payment_form(
            "<form action='/pay' method='post'><input name='x' value='1'></form>",
        )
        .unwrap();
        assert_eq!(v["fields"]["x"], "1");
    }
    #[test]
    fn mutating_operations_classified() {
        let ops = operations();
        for name in [
            "library.book",
            "library.cancel",
            "library.room-book",
            "sports.book",
            "sports.payment-start",
        ] {
            assert_eq!(
                ops.as_array()
                    .unwrap()
                    .iter()
                    .find(|v| v["name"] == name)
                    .unwrap()["mutating"],
                true
            );
        }
    }
    #[test]
    fn gbk_encodes_chinese() {
        let p = BTreeMap::from([("xm".into(), "清华大学".into())]);
        assert_eq!(gbk_form(&p).unwrap(), "xm=%C7%E5%BB%AA%B4%F3%D1%A7");
    }
}

// Only known school/payment hosts may receive server-provided payment forms.
fn payment_target(action: &str, base: &str) -> Result<String> {
    let u = reqwest::Url::parse(base)?.join(action)?;
    let h = u.host_str().unwrap_or("");
    if u.scheme() != "https"
        || !(h == "tsinghua.edu.cn"
            || h.ends_with(".tsinghua.edu.cn")
            || h == "alipay.com"
            || h.ends_with(".alipay.com"))
    {
        bail!("payment form points outside supported HTTPS school/Alipay hosts; continue manually");
    }
    Ok(u.to_string())
}
async fn payment_qr(c: &Client, first_html: &str) -> Result<Value> {
    let first = parse_payment_form(first_html)?;
    let target = payment_target(required_form(&first, "action")?, SPORT)?;
    let payment_html = form(c, &target, &first["fields"]).await?;
    let re = Regex::new(r"var id = '(.*?)';\s*var token = '(.*?)';")?;
    let caps = re
        .captures(&payment_html)
        .ok_or_else(|| anyhow!("payment check credentials absent in response"))?;
    let base="https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421f6f60c93293c615e7b469dbf915b243daf0f96e17deaf447b4/zjjsfw/zjjs";
    let check: Value = serde_json::from_str(
        &form(
            c,
            &format!("{base}/check.do"),
            &json!({"id":&caps[1],"token":&caps[2]}),
        )
        .await?,
    )?;
    if check["code"] != "0" {
        bail!("payment eligibility check failed");
    }
    let doc = Html::parse_document(&payment_html);
    let mut fields = doc
        .select(&sel("#payForm input[name]"))
        .map(|i| {
            (
                i.value().attr("name").unwrap().to_owned(),
                json!(i.value().attr("value").unwrap_or("")),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    if fields.is_empty() {
        bail!("payment page has no payForm fields");
    }
    fields.insert("channelId".into(), json!("0101"));
    let provider_html = form(c, &format!("{base}/webPay.do"), &Value::Object(fields)).await?;
    let provider = parse_payment_form(&provider_html)?;
    let provider_target = payment_target(
        required_form(&provider, "action")?,
        &format!("{base}/webPay.do"),
    )?;
    let content = provider["fields"]["biz_content"]
        .as_str()
        .ok_or_else(|| anyhow!("payment form missing biz_content"))?;
    let qr_html = form(c, &provider_target, &json!({"biz_content":content})).await?;
    let doc = Html::parse_document(&qr_html);
    let qr = doc
        .select(&sel("input[name=qrCode]"))
        .next()
        .and_then(|i| i.value().attr("value"))
        .ok_or_else(|| anyhow!("payment provider returned no QR code"))?;
    Ok(
        json!({"payment_completed":false,"qr_url":qr,"qr_code":qr.rsplit('/').next(),"instruction":"Review amount and complete payment yourself in Alipay."}),
    )
}
fn required_form<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v[k].as_str()
        .ok_or_else(|| anyhow!("payment form missing {k}"))
}

fn validate_room_times(start: &str, end: &str) -> Result<()> {
    let begin = chrono::NaiveDateTime::parse_from_str(start, "%Y-%m-%d %H:%M")
        .context("start must be YYYY-MM-DD HH:MM (Beijing time)")?;
    let finish = chrono::NaiveDateTime::parse_from_str(end, "%Y-%m-%d %H:%M")
        .context("end must be YYYY-MM-DD HH:MM (Beijing time)")?;
    if finish <= begin {
        bail!("end must be after start");
    }
    Ok(())
}
#[cfg(test)]
mod safety_tests {
    use super::*;
    #[test]
    fn payment_destinations_fail_closed() {
        assert!(payment_target("https://evil.example/collect", SPORT).is_err());
        assert!(payment_target("http://card.tsinghua.edu.cn/pay", SPORT).is_err());
        assert!(payment_target("https://openapi.alipay.com/gateway.do", SPORT).is_ok());
        assert!(payment_target("https://tsinghua.edu.cn.evil.example/pay", SPORT).is_err());
    }
    #[test]
    fn validate_reservation_times() {
        assert!(validate_room_times("2026-11-12 10:00", "2026-11-12 12:00").is_ok());
        assert!(validate_room_times("2026-11-12 12:00", "2026-11-12 10:00").is_err());
        assert!(validate_room_times("2026-02-30 10:00", "2026-03-01 10:00").is_err());
    }
}
