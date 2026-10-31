//! Native Rust port of the API contracts used by learnX / thu-learn-lib (MIT).
//! See docs/learn-coverage.md and third-party license notices.
use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use reqwest::{multipart, Client, Method, Url};
use scraper::{Html, Selector};
use serde_json::{json, Value};
use std::path::Path;

const BASE: &str = "https://learn.tsinghua.edu.cn";
const REGISTRAR: &str = "https://zhjw.cic.tsinghua.edu.cn";
const TYPES: &[(&str, &str)] = &[
    ("notification", "KCGG"),
    ("file", "KCKJ"),
    ("homework", "KCZY"),
    ("discussion", "KCTL"),
    ("question", "KCDY"),
    ("questionnaire", "KCWJ"),
];

pub fn operations() -> Value {
    let ops = [
        (
            "user",
            false,
            "Name and department from the authenticated homepage",
            "",
            "role",
        ),
        ("semesters", false, "Available semester identifiers", "", ""),
        ("current-semester", false, "Current semester dates", "", ""),
        (
            "courses",
            false,
            "Courses with time/location enrichment",
            "semester",
            "role,language",
        ),
        (
            "course-location",
            false,
            "Course time and location",
            "course",
            "",
        ),
        (
            "notifications",
            false,
            "Current and expired notices with attachments",
            "course",
            "role",
        ),
        (
            "notification-detail",
            false,
            "Notice HTML and attachment links (may mark notice read)",
            "course,id",
            "role",
        ),
        (
            "files",
            false,
            "Course files and categories (upstream size limit 200)",
            "course",
            "role",
        ),
        (
            "file-categories",
            false,
            "Course file categories",
            "course",
            "role",
        ),
        (
            "files-by-category",
            false,
            "Files within a category",
            "course,category",
            "role",
        ),
        (
            "file-download",
            false,
            "Download an authenticated course file to a new local file",
            "id,output",
            "role",
        ),
        (
            "file-preview",
            false,
            "Fetch supported file preview HTML",
            "id,type",
            "role,first_page",
        ),
        (
            "homework",
            false,
            "All new/submitted/graded homework with details",
            "course",
            "role",
        ),
        (
            "homework-detail",
            false,
            "Homework page, attachments and description",
            "course,id,base_id",
            "excellent",
        ),
        (
            "excellent-homework",
            false,
            "Excellent homework examples",
            "course",
            "",
        ),
        (
            "homework-submit",
            true,
            "Submit content and optional attachment",
            "id",
            "content,attachment,remove_attachment",
        ),
        (
            "homework-download",
            false,
            "Download a homework attachment to a new local file",
            "course,id,output",
            "",
        ),
        (
            "discussions",
            false,
            "Course discussion threads",
            "course",
            "role",
        ),
        (
            "questions",
            false,
            "Answered questions (all in teacher mode)",
            "course",
            "role",
        ),
        (
            "questionnaires",
            false,
            "Ongoing and ended questionnaires with questions",
            "course",
            "",
        ),
        (
            "questionnaire-detail",
            false,
            "Questions and options in a questionnaire",
            "course,id",
            "",
        ),
        ("favorites", false, "Saved content items", "", "course,type"),
        ("favorite-add", true, "Save a content item", "id,type", ""),
        ("favorite-remove", true, "Remove a saved item", "id", ""),
        ("favorite-pin", true, "Pin a saved item", "id", ""),
        ("favorite-unpin", true, "Unpin a saved item", "id", ""),
        (
            "comments",
            false,
            "Personal content annotations",
            "",
            "course,type",
        ),
        (
            "comment-set",
            true,
            "Set or clear a personal annotation",
            "id,type,content",
            "",
        ),
        (
            "courses-sort",
            true,
            "Set course display order",
            "courses",
            "",
        ),
        (
            "calendar",
            false,
            "Registrar timetable within a date interval (maximum 29 days)",
            "start,end",
            "graduate",
        ),
        ("language", false, "Read homepage language", "", ""),
        ("language-set", true, "Set website language", "language", ""),
        (
            "logout",
            true,
            "Invalidate the learn.tsinghua.edu.cn session",
            "",
            "",
        ),
        (
            "all-contents",
            false,
            "Aggregate selected content for multiple course IDs",
            "courses,type",
            "role",
        ),
        (
            "coursex-upload",
            true,
            "Share course metadata with CourseX using your own token",
            "courses,refresh_token",
            "",
        ),
    ];
    Value::Array(ops.into_iter().map(|(name,mutating,description,required,optional)|json!({"name":name,"mutating":mutating,"description":description,"required":csv(required),"optional":csv(optional),"upstream":"robertying/thu-learn-lib@1b79b984f34a6f1ca0532beaf59abac55db7f52c (coursex-upload: learnX/src/helpers/coursex.ts)"})).collect())
}
fn csv(s: &str) -> Vec<&str> {
    s.split(',').filter(|s| !s.is_empty()).collect()
}
fn arg<'a>(a: &'a Value, k: &str) -> Result<&'a str> {
    a.get(k)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Missing nonempty string argument '{k}'"))
}
fn opt<'a>(a: &'a Value, k: &str, d: &'a str) -> &'a str {
    a.get(k).and_then(Value::as_str).unwrap_or(d)
}
fn role(a: &Value) -> Result<&str> {
    match opt(a, "role", "student") {
        "student" => Ok("student"),
        "teacher" => Ok("teacher"),
        _ => bail!("role must be student or teacher"),
    }
}
fn content_type(a: &Value, optional: bool) -> Result<&'static str> {
    if optional && a.get("type").is_none() {
        return Ok("ALL");
    };
    let t = arg(a, "type")?;
    TYPES
        .iter()
        .find(|(k, _)| *k == t)
        .map(|(_, v)| *v)
        .ok_or_else(|| {
            anyhow!(
                "type must be notification, file, homework, discussion, question or questionnaire"
            )
        })
}
fn text(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| {
            v.get(k)
                .filter(|v| !v.is_null())
                .map(Value::to_string)
                .unwrap_or_default()
        })
}
fn decoded(v: &Value, k: &str) -> String {
    html_decode(&text(v, k))
}
fn html_decode(s: &str) -> String {
    let html = Html::parse_fragment(s);
    html.root_element().text().collect::<String>()
}
fn b64(v: &Value, k: &str) -> String {
    base64::engine::general_purpose::STANDARD
        .decode(text(v, k))
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .unwrap_or_default()
}
fn yes(v: &Value, k: &str) -> bool {
    matches!(v.get(k),Some(Value::String(s)) if s=="是"||s=="1")
        || v.get(k) == Some(&json!(1))
        || v.get(k) == Some(&json!(true))
}
fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("static CSS selector")
}
fn url(path: &str, query: &[(&str, &str)], csrf: Option<&str>) -> Result<Url> {
    let mut u = Url::parse(BASE)?.join(path)?;
    {
        let mut q = u.query_pairs_mut();
        for (k, v) in query {
            q.append_pair(k, v);
        }
        if let Some(t) = csrf {
            q.append_pair("_csrf", t);
        }
    }
    Ok(u)
}
fn mp(fields: &[(&str, String)]) -> multipart::Form {
    fields.iter().fold(multipart::Form::new(), |f, (k, v)| {
        f.text((*k).to_owned(), v.to_owned())
    })
}
fn page_form(course: Option<&str>) -> multipart::Form {
    mp(&[(
        "aoData",
        json!(course
            .map(|c| vec![json!({"name":"wlkcid","value":c})])
            .unwrap_or_default())
        .to_string(),
    )])
}
async fn response_text(r: reqwest::Response) -> Result<String> {
    let status = r.status();
    let host = r.url().host_str().unwrap_or("").to_owned();
    let path = r.url().path().to_owned();
    if !status.is_success() {
        bail!("Server HTTP {} (response body omitted)", status.as_u16());
    }
    if host == "id.tsinghua.edu.cn"
        || path.contains("j_spring_security_check")
        || path.contains("/login")
    {
        bail!("Authentication expired; log in or import fresh cookies")
    }
    let t = r.text().await?;
    if t.contains("id=\"i_user\"") || t.contains("name=\"i_user\"") {
        bail!("Received login page; authentication required")
    }
    Ok(t)
}
async fn json_response(r: reqwest::Response) -> Result<Value> {
    let s = response_text(r).await?;
    let v: Value =
        serde_json::from_str(&s).context("Expected JSON response; session may be expired")?;
    if v.get("result") == Some(&json!("error")) || v.get("message") == Some(&json!("error")) {
        bail!("Server reported operation failure (response omitted)")
    }
    Ok(v)
}
async fn get(c: &Client, path: &str, q: &[(&str, &str)], token: &str) -> Result<Value> {
    json_response(c.get(url(path, q, Some(token))?).send().await?).await
}
async fn post(
    c: &Client,
    path: &str,
    q: &[(&str, &str)],
    form: multipart::Form,
    token: &str,
) -> Result<Value> {
    json_response(
        c.post(url(path, q, Some(token))?)
            .multipart(form)
            .send()
            .await?,
    )
    .await
}
fn rows(v: &Value) -> Result<Vec<Value>> {
    if let Some(a) = v.as_array() {
        return Ok(a.clone());
    }
    for p in [
        "/resultList",
        "/object/aaData",
        "/object/resultsList",
        "/object/rows",
        "/object",
    ] {
        if let Some(a) = v.pointer(p).and_then(Value::as_array) {
            return Ok(a.clone());
        }
    }
    bail!("Response has no recognized result array")
}
fn require_success(v: Value) -> Result<Value> {
    if v.get("result") == Some(&json!("success")) || v.get("message") == Some(&json!("success")) {
        Ok(v)
    } else {
        bail!("Server did not confirm operation success")
    }
}
pub fn parse_csrf(html: &str) -> Result<String> {
    let re = regex::Regex::new(r#"(?:[?&]|&amp;)_csrf=([^\s\"'&<>]+)"#)?;
    if let Some(c) = re.captures(html) {
        return Ok(c[1].to_owned());
    }
    let doc = Html::parse_document(html);
    for e in doc.select(&sel("input[name='_csrf'],meta[name='_csrf']")) {
        if let Some(v) = e
            .value()
            .attr("value")
            .or_else(|| e.value().attr("content"))
        {
            if !v.is_empty() {
                return Ok(v.to_owned());
            }
        }
    }
    bail!("No CSRF token on Learn homepage; authenticate first")
}
async fn csrf(c: &Client, a: &Value) -> Result<String> {
    if let Some(t) = a.get("csrf").and_then(Value::as_str) {
        if !t.is_empty() {
            return Ok(t.to_owned());
        }
    }
    let html = response_text(
        c.get(format!("{BASE}/f/wlxt/index/course/student/"))
            .send()
            .await?,
    )
    .await?;
    parse_csrf(&html)
}
fn preview(id: &str, kind: &str, role: &str, first: bool) -> Result<String> {
    let mk = match kind {
        "file" => "mk_kcwj",
        "homework" => "mk_kczy",
        "notification" => "mk_kcgg",
        _ => bail!("Preview type must be file, homework or notification"),
    };
    Ok(url(
        &format!("/f/wlxt/kc/wj_wjb/{role}/beforePlay"),
        &[
            ("wjid", id),
            ("mk", mk),
            ("browser", "-1"),
            ("sfgk", "0"),
            ("pageType", if first { "first" } else { "all" }),
        ],
        None,
    )?
    .into())
}
fn download_url(id: &str, r: &str) -> Result<String> {
    Ok(url(
        &format!("/b/wlxt/kj/wlkc_kjxxb/{r}/downloadFile"),
        &[("sfgk", "0"), ("wjid", id)],
        None,
    )?
    .into())
}
fn file_record(f: &Value, r: &str, array: bool) -> Result<Value> {
    let (id, fid, title, description, size, uploaded, published, ftype) = if array {
        (
            f[0].clone(),
            f[7].clone(),
            html_decode(f[1].as_str().unwrap_or("")),
            html_decode(f[5].as_str().unwrap_or("")),
            f[9].clone(),
            f[6].clone(),
            f[10].clone(),
            f[13].clone(),
        )
    } else {
        (
            f["kjxxid"].clone(),
            f["wjid"].clone(),
            decoded(f, "bt"),
            decoded(f, "ms"),
            f["wjdx"].clone(),
            f["scsj"].clone(),
            f["scsj"].clone(),
            f["wjlx"].clone(),
        )
    };
    let fs = fid
        .as_str()
        .ok_or_else(|| anyhow!("File response missing wjid"))?;
    Ok(
        json!({"id":id,"fileId":fid,"title":title,"description":description,"rawSize":size,"uploadTime":uploaded,"publishTime":published,"fileType":ftype,"downloadUrl":download_url(fs,r)?,"previewUrl":preview(fs,"file",r,true)?,"upstream":f}),
    )
}
fn html_detail(html: &str) -> Result<Value> {
    let d = Html::parse_document(html);
    let mut links = Vec::new();
    for e in d.select(&sel("a[href]")) {
        if let Some(h) = e.value().attr("href") {
            if h.contains("download") || h.contains("wjid=") || h.contains("fileId=") {
                let u = Url::parse(BASE)?.join(h)?;
                links.push(json!({"name":e.text().collect::<String>().trim(),"url":u.as_str()}))
            }
        }
    }
    Ok(json!({"html":html,"attachments":links}))
}
async fn homework_detail(
    c: &Client,
    course: &str,
    id: &str,
    base: &str,
    excellent: bool,
    t: &str,
) -> Result<Value> {
    let view = if excellent { "viewYxzy" } else { "viewCj" };
    let html = response_text(
        c.get(url(
            &format!("/f/wlxt/kczy/zy/student/{view}"),
            &[("wlkcid", course), ("xszyid", id)],
            Some(t),
        )?)
        .send()
        .await?,
    )
    .await?;
    let mut v = html_detail(&html.replace(t, "[REDACTED]"))?;
    let doc = Html::parse_document(&html);
    let values: Vec<_> = doc
        .select(&sel("div.list.calendar.clearfix > div.fl.right > div.c55"))
        .map(|e| e.inner_html())
        .collect();
    v["description"] = values
        .first()
        .cloned()
        .map(Value::String)
        .unwrap_or(Value::Null);
    v["answerContent"] = values
        .get(1)
        .cloned()
        .map(Value::String)
        .unwrap_or(Value::Null);
    let boxes: Vec<_> = doc.select(&sel("div.boxbox")).collect();
    v["submittedContent"] = boxes
        .get(1)
        .and_then(|e| e.select(&sel("div.right")).nth(2))
        .map(|e| Value::String(e.inner_html()))
        .unwrap_or(Value::Null);
    if !base.is_empty() {
        let j = post(
            c,
            "/b/wlxt/kczy/zy/student/detail",
            &[],
            mp(&[("id", base.to_owned())]),
            t,
        )
        .await?;
        require_success(j.clone())?;
        v["description"] = j["msg"].clone();
    }
    Ok(v)
}
async fn homework(c: &Client, a: &Value, t: &str) -> Result<Value> {
    let course = arg(a, "course")?;
    let r = role(a)?;
    let mut result = Vec::new();
    if r == "teacher" {
        let v = post(
            c,
            "/b/wlxt/kczy/zy/teacher/pageList",
            &[],
            page_form(Some(course)),
            t,
        )
        .await?;
        for h in rows(&v)? {
            result.push(json!({"id":h["zyid"],"title":decoded(&h,"bt"),"description":b64(&h,"nr"),"deadline":h["jzsj"],"startTime":h["kssj"],"gradedCount":h["ypys"],"submittedCount":h["yjs"],"unsubmittedCount":h["wjs"],"upstream":h}))
        }
    } else {
        for (suffix, submitted, graded) in [
            ("zyListWj", false, false),
            ("zyListYjwg", true, false),
            ("zyListYpg", true, true),
        ] {
            let v = post(
                c,
                &format!("/b/wlxt/kczy/zy/student/{suffix}"),
                &[],
                page_form(Some(course)),
                t,
            )
            .await?;
            for h in rows(&v)? {
                let id = text(&h, "xszyid");
                let base = text(&h, "zyid");
                let detail = homework_detail(c, course, &id, &base, false, t).await?;
                result.push(json!({"id":id,"baseId":base,"title":decoded(&h,"bt"),"deadline":h["jzsj"],"lateSubmissionDeadline":h["bjjzsj"],"submitted":submitted,"graded":graded,"grade":h["cj"],"gradeContent":h["pynr"],"detail":detail,"upstream":h}))
            }
        }
    }
    Ok(Value::Array(result))
}
async fn download(c: &Client, u: Url, output: &str) -> Result<Value> {
    use std::io::Write;
    let mut r = c.get(u).send().await?;
    if !r.status().is_success() {
        bail!("Download HTTP {}", r.status().as_u16())
    }
    if r.url().host_str() == Some("id.tsinghua.edu.cn") {
        bail!("Download redirected to login")
    }
    if r.headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|x| x.to_str().ok())
        .is_some_and(|s| s.contains("text/html"))
    {
        bail!("Download returned HTML; refusing to save possible login/error page")
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut f = options
        .open(output)
        .context("Cannot create download destination (existing files are not overwritten)")?;
    let mut count = 0u64;
    let result: Result<()> = async {
        while let Some(b) = r.chunk().await? {
            f.write_all(&b)?;
            count += b.len() as u64;
        }
        f.sync_all()?;
        Ok(())
    }
    .await;
    if let Err(e) = result {
        drop(f);
        let _ = std::fs::remove_file(output);
        return Err(e);
    }
    Ok(json!({"output":output,"bytes":count}))
}

pub async fn execute(c: &Client, op: &str, a: &Value) -> Result<Value> {
    if !operations()
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["name"] == op)
    {
        bail!("Unknown learn operation '{op}'")
    }
    if op == "coursex-upload" {
        return coursex(c, a).await;
    }
    if op == "logout" {
        let response = c
            .post(format!("{BASE}/f/j_spring_security_logout"))
            .send()
            .await?;
        if !response.status().is_success() {
            bail!(
                "Remote logout failed with HTTP {}",
                response.status().as_u16()
            )
        };
        return Ok(json!({"loggedOut":true}));
    }
    let t = csrf(c, a).await?;
    let r = role(a)?;
    match op {
        "user" | "language" => {
            let html = response_text(
                c.get(url(&format!("/f/wlxt/index/course/{r}/"), &[], Some(&t))?)
                    .send()
                    .await?,
            )
            .await?;
            let d = Html::parse_document(&html);
            if op == "language" {
                let re = regex::Regex::new(r"languagejs\?lang=(zh|en)")?;
                return Ok(json!({"language":re.captures(&html).map(|c|c[1].to_owned())}));
            }
            Ok(
                json!({"name":d.select(&sel("a.user-log")).flat_map(|e|e.text()).collect::<String>().trim(),"department":d.select(&sel(".fl.up-img-info p:nth-child(2) label")).flat_map(|e|e.text()).collect::<String>().trim()}),
            )
        }
        "semesters" => {
            let v = get(c, "/b/wlxt/kc/v_wlkc_xs_xktjb_coassb/queryxnxq", &[], &t).await?;
            Ok(Value::Array(
                rows(&v)?.into_iter().filter(|s| !s.is_null()).collect(),
            ))
        }
        "current-semester" => {
            let v = get(
                c,
                "/b/kc/zhjw_v_code_xnxq/getCurrentAndNextSemester",
                &[],
                &t,
            )
            .await?;
            require_success(v.clone())?;
            let x = &v["result"];
            Ok(json!({"id":x["id"],"startDate":x["kssj"],"endDate":x["jssj"],"upstream":x}))
        }
        "courses" => {
            let sem = arg(a, "semester")?;
            let lang = opt(a, "language", "zh");
            if !["zh", "en"].contains(&lang) {
                bail!("language must be zh or en")
            };
            let path = if r == "student" {
                format!(
                    "/b/wlxt/kc/v_wlkc_xs_xkb_kcb_extend/student/loadCourseBySemesterId/{}/{lang}",
                    segment(sem)?
                )
            } else {
                format!("/b/kc/v_wlkc_kcb/queryAsorCoCourseList/{}/0", segment(sem)?)
            };
            let v = get(c, &path, &[], &t).await?;
            require_success(v.clone())?;
            let mut courses = Vec::new();
            for x in rows(&v)? {
                let id = text(&x, "wlkcid");
                let loc = get(c, "/b/kc/v_wlkc_xk_sjddb/detail", &[("id", &id)], &t).await;
                courses.push(json!({"id":id,"name":decoded(&x,"zywkcm"),"chineseName":decoded(&x,"kcm"),"englishName":decoded(&x,"ywkcm"),"teacherName":x["jsm"],"teacherNumber":x["jsh"],"courseNumber":x["kch"],"courseIndex":x["kxh"],"semesterId":sem,"courseType":r,"timeAndLocation":loc.as_ref().ok(),"locationFetchFailed":loc.is_err(),"upstream":x}));
            }
            Ok(Value::Array(courses))
        }
        "course-location" => {
            get(
                c,
                "/b/kc/v_wlkc_xk_sjddb/detail",
                &[("id", arg(a, "course")?)],
                &t,
            )
            .await
        }
        "notifications" => {
            let course = arg(a, "course")?;
            let mut out = Vec::new();
            for suffix in ["Wgq", "Ygq"] {
                let kind = if r == "student" {
                    "pageListXsby"
                } else {
                    "pageListby"
                };
                let v = post(
                    c,
                    &format!("/b/wlxt/kcgg/wlkc_ggb/{r}/{kind}{suffix}"),
                    &[],
                    page_form(Some(course)),
                    &t,
                )
                .await?;
                for n in rows(&v)? {
                    let id = text(&n, "ggid");
                    let attachment_name = text(&n, if r == "student" { "fjmc" } else { "fjbt" });
                    let attachment = if !attachment_name.is_empty() {
                        Some(notice_detail(c, course, &id, r, &t).await?)
                    } else {
                        None
                    };
                    out.push(json!({"id":id,"title":decoded(&n,"bt"),"content":b64(&n,"ggnr"),"hasRead":yes(&n,"sfyd"),"publisher":n["fbrxm"],"publishTime":n.get("fbsj").or_else(||n.get("fbsjStr")),"expireTime":n["jzsj"],"isFavorite":yes(&n,"sfsc"),"attachmentDetail":attachment,"upstream":n}));
                }
            }
            Ok(Value::Array(out))
        }
        "notification-detail" => notice_detail(c, arg(a, "course")?, arg(a, "id")?, r, &t).await,
        "file-categories" => categories(c, arg(a, "course")?, r, &t).await,
        "files" | "files-by-category" => {
            let course = arg(a, "course")?;
            let by = op == "files-by-category";
            let v = if by {
                let cat = arg(a, "category")?;
                if r == "student" {
                    get(
                        c,
                        &format!(
                            "/b/wlxt/kj/wlkc_kjxxb/student/kjxxb/{}/{}",
                            segment(course)?,
                            segment(cat)?
                        ),
                        &[],
                        &t,
                    )
                    .await?
                } else {
                    post(
                        c,
                        "/b/wlxt/kj/v_kjxxb_wjwjb/teacher/pageList",
                        &[],
                        mp(&[(
                            "aoData",
                            json!([{"name":"wlkcid","value":course},{"name":"kjflid","value":cat}])
                                .to_string(),
                        )]),
                        &t,
                    )
                    .await?
                }
            } else {
                let path = if r == "student" {
                    "/b/wlxt/kj/wlkc_kjxxb/student/kjxxbByWlkcidAndSizeForStudent"
                } else {
                    "/b/wlxt/kj/v_kjxxb_wjwjb/teacher/queryByWlkcid"
                };
                get(c, path, &[("wlkcid", course), ("size", "200")], &t).await?
            };
            let result = rows(&v)?
                .iter()
                .map(|f| file_record(f, r, by && r == "student"))
                .collect::<Result<Vec<_>>>()?;
            if by {
                Ok(json!(result))
            } else {
                Ok(
                    json!({"files":result,"categories":categories(c,course,r,&t).await?,"serverSizeLimit":200}),
                )
            }
        }
        "file-download" => {
            download(
                c,
                url(
                    &format!("/b/wlxt/kj/wlkc_kjxxb/{r}/downloadFile"),
                    &[("sfgk", "0"), ("wjid", arg(a, "id")?)],
                    Some(&t),
                )?,
                arg(a, "output")?,
            )
            .await
        }
        "homework-download" => {
            download(
                c,
                url(
                    &format!(
                        "/b/wlxt/kczy/zy/student/downloadFile/{}/{}",
                        segment(arg(a, "course")?)?,
                        segment(arg(a, "id")?)?
                    ),
                    &[],
                    Some(&t),
                )?,
                arg(a, "output")?,
            )
            .await
        }
        "file-preview" => {
            let mut u = Url::parse(&preview(
                arg(a, "id")?,
                arg(a, "type")?,
                r,
                a.get("first_page").and_then(Value::as_bool).unwrap_or(true),
            )?)?;
            u.query_pairs_mut().append_pair("_csrf", &t);
            let html = response_text(c.get(u).send().await?).await?;
            html_detail(&html.replace(&t, "[REDACTED]"))
        }
        "homework" => homework(c, a, &t).await,
        "homework-detail" => {
            homework_detail(
                c,
                arg(a, "course")?,
                arg(a, "id")?,
                arg(a, "base_id")?,
                a.get("excellent").and_then(Value::as_bool).unwrap_or(false),
                &t,
            )
            .await
        }
        "excellent-homework" => {
            let course = arg(a, "course")?;
            let v = post(
                c,
                "/b/wlxt/kczy/zy/student/yxzylist",
                &[],
                page_form(Some(course)),
                &t,
            )
            .await?;
            let mut out = Vec::new();
            for h in rows(&v)? {
                let id = text(&h, "xszyid");
                let base = text(&h, "zyid");
                out.push(json!({"id":id,"baseId":base,"title":decoded(&h,"bt"),"author":h["cy"],"anonymous":yes(&h,"sfzm"),"detail":homework_detail(c,course,&id,&base,true,&t).await?,"upstream":h}));
            }
            Ok(json!(out))
        }
        "homework-submit" => {
            let mut f = mp(&[
                ("xszyid", arg(a, "id")?.to_owned()),
                ("zynr", opt(a, "content", "").to_owned()),
                (
                    "isDeleted",
                    if a.get("remove_attachment")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        "1"
                    } else {
                        "0"
                    }
                    .to_owned(),
                ),
            ]);
            if let Some(p) = a.get("attachment").and_then(Value::as_str) {
                let path = Path::new(p);
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| anyhow!("Attachment has no valid filename"))?;
                let bytes = std::fs::read(path).context("Cannot read attachment")?;
                f = f.part(
                    "fileupload",
                    multipart::Part::bytes(bytes).file_name(name.to_owned()),
                );
            } else {
                f = f.text("fileupload", "undefined");
            }
            require_success(post(c, "/b/wlxt/kczy/zy/student/tjzy", &[], f, &t).await?)
        }
        "discussions" | "questions" => {
            let course = arg(a, "course")?;
            let suffix = if op == "discussions" {
                "kctlList"
            } else {
                "kcdyList"
            };
            let v = get(
                c,
                &format!("/b/wlxt/bbs/bbs_tltb/{r}/{suffix}"),
                &[("wlkcid", course), ("size", "200")],
                &t,
            )
            .await?;
            Ok(
                json!({"items":rows(&v)?.into_iter().map(|x|json!({"id":x["id"],"title":decoded(&x,"bt"),"publisherName":x["fbrxm"],"publishTime":x["fbsj"],"lastReplyTime":x["zhhfsj"],"lastReplierName":x["zhhfrxm"],"replyCount":x["hfcs"],"boardId":x["bqid"],"question":b64(&x,"wtnr"),"upstream":x})).collect::<Vec<_>>(),"serverSizeLimit":200}),
            )
        }
        "questionnaires" => {
            let course = arg(a, "course")?;
            let mut out = Vec::new();
            for suffix in ["pageListWks", "pageListYjs"] {
                let v = post(
                    c,
                    &format!("/b/wlxt/kcwj/wlkc_wjb/student/{suffix}"),
                    &[],
                    page_form(Some(course)),
                    &t,
                )
                .await?;
                for q in rows(&v)? {
                    let id = text(&q, "wjid");
                    out.push(json!({"id":id,"title":decoded(&q,"wjbt"),"type":q["wjlx"],"startTime":q["kssj"],"endTime":q["jssj"],"submitTime":q["tjsj"],"detail":qnr_detail(c,course,&id,&t).await?,"upstream":q}));
                }
            }
            Ok(json!(out))
        }
        "questionnaire-detail" => qnr_detail(c, arg(a, "course")?, arg(a, "id")?, &t).await,
        "favorites" | "comments" => {
            let typ = content_type(a, true)?;
            let path = if op == "favorites" {
                "/b/xt/wlkc_xsscb/student/pageList"
            } else {
                "/b/wlxt/xt/wlkc_xsbjb/student/pageList"
            };
            let v = post(
                c,
                path,
                &[("ywlx", typ)],
                page_form(a.get("course").and_then(Value::as_str)),
                &t,
            )
            .await?;
            Ok(json!(rows(&v)?))
        }
        "favorite-add" => require_success(
            get(
                c,
                "/b/xt/wlkc_xsscb/student/add",
                &[("ywid", arg(a, "id")?), ("ywlx", content_type(a, false)?)],
                &t,
            )
            .await?,
        ),
        "favorite-remove" => require_success(
            get(
                c,
                "/b/xt/wlkc_xsscb/student/delete",
                &[("ywid", arg(a, "id")?)],
                &t,
            )
            .await?,
        ),
        "favorite-pin" | "favorite-unpin" => {
            let path = if op == "favorite-pin" {
                "/b/xt/wlkc_xsscb/student/addZd"
            } else {
                "/b/xt/wlkc_xsscb/student/delZd"
            };
            require_success(
                post(c, path, &[], mp(&[("ywid", arg(a, "id")?.to_owned())]), &t).await?,
            )
        }
        "comment-set" => require_success(
            post(
                c,
                "/b/wlxt/xt/wlkc_xsbjb/add",
                &[],
                mp(&[
                    ("ywlx", content_type(a, false)?.to_owned()),
                    ("ywid", arg(a, "id")?.to_owned()),
                    (
                        "bznr",
                        a.get("content")
                            .and_then(Value::as_str)
                            .context("Missing content (empty string clears annotation)")?
                            .to_owned(),
                    ),
                ]),
                &t,
            )
            .await?,
        ),
        "courses-sort" => {
            let ids = a
                .get("courses")
                .and_then(Value::as_array)
                .context("courses must be an array of IDs")?;
            let body = ids
                .iter()
                .enumerate()
                .map(|(i, id)| {
                    let id = id.as_str().context("course IDs must be strings")?;
                    Ok(json!({"wlkcid":id,"xh":i+1}))
                })
                .collect::<Result<Vec<_>>>()?;
            require_success(
                json_response(
                    c.post(url("/b/wlxt/kc/wlkc_kcpxb/addorUpdate", &[], Some(&t))?)
                        .json(&body)
                        .send()
                        .await?,
                )
                .await?,
            )
        }
        "language-set" => {
            let l = match arg(a, "language")? {
                "zh" => "zh_CN",
                "en" => "en_US",
                _ => bail!("language must be zh or en"),
            };
            let _ = response_text(
                c.request(
                    Method::POST,
                    url(
                        "/f/wlxt/common/language",
                        &[("websiteShowLanguage", l)],
                        Some(&t),
                    )?,
                )
                .send()
                .await?,
            )
            .await?;
            Ok(json!({"language":arg(a,"language")?}))
        }
        "calendar" => calendar(c, a, &t).await,
        "all-contents" => {
            let ids = a
                .get("courses")
                .and_then(Value::as_array)
                .context("courses must be an array")?;
            let method = match arg(a, "type")? {
                "notification" => "notifications",
                "file" => "files",
                "homework" => "homework",
                "discussion" => "discussions",
                "question" => "questions",
                "questionnaire" => "questionnaires",
                _ => bail!("Unsupported content type"),
            };
            let mut out = serde_json::Map::new();
            for id in ids {
                let id = id.as_str().context("course IDs must be strings")?;
                let mut sub = a.clone();
                sub["course"] = json!(id);
                sub["csrf"] = json!(t);
                out.insert(id.to_owned(), Box::pin(execute(c, method, &sub)).await?);
            }
            Ok(Value::Object(out))
        }
        _ => bail!("Unknown learn operation '{op}'"),
    }
}
fn segment(s: &str) -> Result<String> {
    if s.is_empty() || s.contains(['/', '\\', '?', '#']) || s == "." || s == ".." {
        bail!("Invalid identifier path segment")
    };
    let mut u = Url::parse(BASE)?;
    u.path_segments_mut()
        .map_err(|_| anyhow!("Invalid base URL"))?
        .push(s);
    Ok(u.path().trim_start_matches('/').to_owned())
}
async fn notice_detail(c: &Client, course: &str, id: &str, r: &str, t: &str) -> Result<Value> {
    let kind = if r == "student" {
        "beforeViewXs"
    } else {
        "beforeViewJs"
    };
    let html = response_text(
        c.get(url(
            &format!("/f/wlxt/kcgg/wlkc_ggb/{r}/{kind}"),
            &[("wlkcid", course), ("id", id)],
            Some(t),
        )?)
        .send()
        .await?,
    )
    .await?;
    html_detail(&html.replace(t, "[REDACTED]"))
}
async fn categories(c: &Client, course: &str, r: &str, t: &str) -> Result<Value> {
    let v = get(
        c,
        &format!("/b/wlxt/kj/wlkc_kjflb/{r}/pageList"),
        &[("wlkcid", course)],
        t,
    )
    .await?;
    Ok(json!(rows(&v)?
        .into_iter()
        .map(|x| json!({"id":x["kjflid"],"title":decoded(&x,"bt"),"creationTime":x["czsj"]}))
        .collect::<Vec<_>>()))
}
async fn qnr_detail(c: &Client, course: &str, id: &str, t: &str) -> Result<Value> {
    let v = post(
        c,
        "/b/wlxt/kcwj/wlkc_wjb/student/getWjnr",
        &[],
        mp(&[("wlkcid", course.to_owned()), ("wjid", id.to_owned())]),
        t,
    )
    .await?;
    Ok(json!(rows(&v)?.into_iter().map(|x|json!({"id":x["wtid"],"index":x["wtbh"],"type":x["type"],"required":yes(&x,"require"),"title":decoded(&x,"wtbt"),"score":x["wtfz"],"options":x["list"],"upstream":x})).collect::<Vec<_>>()))
}
fn parse_jsonp(s: &str) -> Result<Value> {
    let s = s.trim();
    let prefix = "thu_cli_jsonp(";
    let s = s
        .strip_prefix(prefix)
        .context("Unexpected calendar JSONP callback")?;
    let s = s.strip_suffix(';').unwrap_or(s).trim_end();
    let inner = s.strip_suffix(')').context("Malformed calendar JSONP")?;
    serde_json::from_str(inner).context("Invalid calendar JSONP payload")
}
async fn calendar(c: &Client, a: &Value, t: &str) -> Result<Value> {
    let start = arg(a, "start")?;
    let end = arg(a, "end")?;
    let s = chrono::NaiveDate::parse_from_str(start, "%Y%m%d").context("start must be YYYYMMDD")?;
    let e = chrono::NaiveDate::parse_from_str(end, "%Y%m%d").context("end must be YYYYMMDD")?;
    let days = (e - s).num_days();
    if !(0..29).contains(&days) {
        bail!("Calendar range must cover 1 to 29 inclusive days")
    }
    let ticket = response_text(
        c.post(url("/b/wlxt/common/auth/gnt", &[], Some(t))?)
            .multipart(mp(&[("appId", "ALL_ZHJW".to_owned())]))
            .send()
            .await?,
    )
    .await?;
    let ticket: String =
        serde_json::from_str(&ticket).context("Invalid registrar ticket response")?;
    let mut auth = Url::parse(&format!("{REGISTRAR}/j_acegi_login.do"))?;
    auth.query_pairs_mut()
        .append_pair("url", "/")
        .append_pair("ticket", &ticket);
    let _ = response_text(c.get(auth).send().await?).await?;
    let mut u = Url::parse(&format!("{REGISTRAR}/jxmh_out.do"))?;
    u.query_pairs_mut()
        .append_pair(
            "m",
            if a.get("graduate").and_then(Value::as_bool).unwrap_or(false) {
                "yjs_jxrl_all"
            } else {
                "bks_jxrl_all"
            },
        )
        .append_pair("p_start_date", start)
        .append_pair("p_end_date", end)
        .append_pair("jsoncallback", "thu_cli_jsonp")
        .append_pair("_csrf", t);
    let raw = response_text(c.get(u).send().await?).await?;
    let v = parse_jsonp(&raw)?;
    Ok(json!(rows(&v)?.into_iter().map(|x|json!({"location":x["dd"],"status":x["fl"],"startTime":x["kssj"],"endTime":x["jssj"],"date":x["nq"],"courseName":x["nr"]})).collect::<Vec<_>>()))
}
async fn coursex(c: &Client, a: &Value) -> Result<Value> {
    let token = arg(a, "refresh_token")?;
    let courses = a
        .get("courses")
        .and_then(Value::as_array)
        .context("courses must contain course metadata objects")?;
    let r = json_response(
        c.post("https://tsinghua.app/api/auth/session")
            .json(&json!({"refreshToken":token}))
            .send()
            .await?,
    )
    .await?;
    let access = r
        .get("accessToken")
        .and_then(Value::as_str)
        .context("CourseX did not return access token")?;
    let objects:Vec<_>=courses.iter().map(|x|json!({"id":x["id"],"name":x["chineseName"],"teacher":{"data":{"id":x["teacherNumber"],"name":x["teacherName"]},"on_conflict":{"constraint":"teacher_pkey","update_columns":["name"]}},"time_location":x["timeAndLocation"].to_string(),"semester_id":x["semesterId"],"number":x["courseNumber"],"index":x["courseIndex"],"englishName":x["englishName"]})).collect();
    let query="mutation AddCourses($objects: [course_insert_input!]!) { insert_course(objects: $objects, on_conflict: {constraint: course_pkey, update_columns: [time_location, name, englishName]}) { affected_rows } }";
    let v = json_response(
        c.post("https://api.tsinghua.app/v1/graphql")
            .bearer_auth(access)
            .json(&json!({"query":query,"variables":{"objects":objects}}))
            .send()
            .await?,
    )
    .await?;
    if v.get("errors").is_some() {
        bail!("CourseX GraphQL rejected upload (details omitted)")
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csrf_links_and_meta() {
        assert_eq!(
            parse_csrf(r#"<a href="/x?a=1&_csrf=abc-123">"#).unwrap(),
            "abc-123"
        );
        assert_eq!(
            parse_csrf(r#"<meta name="_csrf" content="t">"#).unwrap(),
            "t"
        );
        assert!(parse_csrf("login").is_err());
    }
    #[test]
    fn query_escaping() {
        let u = url("/test", &[("id", "a&b")], Some("secret")).unwrap();
        assert_eq!(u.query_pairs().find(|(k, _)| k == "id").unwrap().1, "a&b");
    }
    #[test]
    fn no_jsonp_execution() {
        assert_eq!(
            parse_jsonp("thu_cli_jsonp([{\"dd\":\"109\"}]);").unwrap()[0]["dd"],
            "109"
        );
        assert!(parse_jsonp("evil([])").is_err());
        assert!(parse_jsonp("thu_cli_jsonp(process.exit())").is_err());
    }
    #[test]
    fn parse_file_shape() {
        let v = json!({"wjid":"abc","kjxxid":"def","bt":"A &amp; B","wjdx":12});
        let f = file_record(&v, "student", false).unwrap();
        assert_eq!(f["title"], "A & B");
        assert!(f["downloadUrl"].as_str().unwrap().contains("wjid=abc"));
    }
    #[test]
    fn rows_variants() {
        assert_eq!(
            rows(&json!({"object":{"aaData":[1]}})).unwrap(),
            vec![json!(1)]
        );
        assert!(rows(&json!({"result":"success"})).is_err());
    }
    #[test]
    fn inventory_has_mutation_guards() {
        let v = operations();
        assert_eq!(
            v.as_array()
                .unwrap()
                .iter()
                .find(|v| v["name"] == "homework-submit")
                .unwrap()["mutating"],
            true
        );
        assert_eq!(v.as_array().unwrap().len(), 35);
    }
    #[test]
    fn rejects_path_escape() {
        assert!(segment("../login").is_err());
        assert!(segment("a?x=y").is_err());
        assert_eq!(segment("abc").unwrap(), "abc");
    }
    #[tokio::test]
    async fn mock_json_server_failure_and_success() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/good"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({"result":"success","object":{"aaData":[{"id":"fixture"}]}}),
                ),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/bad"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"result":"error","msg":"secret must not leak"})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/login"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<input name=\"i_user\"> "))
            .mount(&server)
            .await;
        let client = Client::new();
        let v = json_response(
            client
                .get(format!("{}/good", server.uri()))
                .send()
                .await
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(rows(&v).unwrap()[0]["id"], "fixture");
        let err = json_response(
            client
                .get(format!("{}/bad", server.uri()))
                .send()
                .await
                .unwrap(),
        )
        .await
        .unwrap_err();
        assert!(!err.to_string().contains("secret must not leak"));
        assert!(json_response(
            client
                .get(format!("{}/login", server.uri()))
                .send()
                .await
                .unwrap()
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn mock_download_never_overwrites() {
        use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(path("/file"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Content-Type", "application/octet-stream")
                    .set_body_bytes(b"fixture".to_vec()),
            )
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.bin");
        let client = Client::new();
        let u = Url::parse(&format!("{}/file", server.uri())).unwrap();
        let v = download(&client, u.clone(), dest.to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(v["bytes"], 7);
        assert_eq!(std::fs::read(&dest).unwrap(), b"fixture");
        assert!(download(&client, u, dest.to_str().unwrap()).await.is_err());
        assert_eq!(std::fs::read(&dest).unwrap(), b"fixture");
    }
}
