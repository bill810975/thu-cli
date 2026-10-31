//! Native unified-identity and WebVPN authentication, ported from thu-info-lib/core.ts
//! and thu-learn-lib. Passwords are encrypted using SM2 before transmission.
use anyhow::{anyhow, bail, Context, Result};
use libsm::sm2::{encrypt::EncryptCtx, signature::SigCtx};
use regex::Regex;
use reqwest::{Client, Url};
use scraper::{Html, Selector};
use serde_json::{json, Value};
use std::{error::Error as StdError, fmt};
use zeroize::Zeroize;

const ID: &str = "https://id.tsinghua.edu.cn";
const CHECK: &str = "https://id.tsinghua.edu.cn/do/off/ui/auth/login/check";
const SAVE_FINGER: &str = "https://id.tsinghua.edu.cn/b/doubleAuth/personal/saveFinger";
const INFO_APP: &str = "10000ea055dd8d81d09d5a1ba55d39ad";
const INFO_HOME: &str = "https://info.tsinghua.edu.cn/f/info/gxfw_fg/common/index";
const LEARN_APP: &str = "bb5df85216504820be7bba2b0ae1535b/0";
const INFO_COOKIE: &str = "https://webvpn.tsinghua.edu.cn/wengine-vpn/cookie?method=get&host=info.tsinghua.edu.cn&scheme=https&path=/f/info/gxfw_fg/common/index";
const INFO_ROAM: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f9f9479369247b59700f81b9991b2631506205de/b/yyfw/vyyfwxx/info/portal_fg/common/onlineAppRedirect";

/// Secrets deliberately do not implement Debug or Serialize.
pub struct LoginOptions {
    pub username: String,
    pub password: String,
    pub fingerprint: String,
    pub service: String,
    pub two_factor_method: Option<String>,
    pub interactive: bool,
    pub trust_device: bool,
    pub use_webvpn: bool,
}
impl Drop for LoginOptions {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}
pub struct RoamOptions {
    pub policy: String,
    pub payload: String,
    pub credentials: Option<LoginOptions>,
}
#[derive(Debug)]
pub struct PasswordRequired;
impl fmt::Display for PasswordRequired {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "identity credential password required")
    }
}
impl StdError for PasswordRequired {}
pub fn is_password_required(error: &anyhow::Error) -> bool {
    error.is::<PasswordRequired>()
}
fn selector(s: &str) -> Selector {
    Selector::parse(s).expect("constant selector")
}
fn text_of(html: &str, css: &str) -> String {
    Html::parse_document(html)
        .select(&selector(css))
        .next()
        .map(|e| e.text().collect::<String>().trim().to_owned())
        .unwrap_or_default()
}
fn anchor(html: &str) -> Result<String> {
    Html::parse_document(html)
        .select(&selector("a[href]"))
        .next()
        .and_then(|e| e.value().attr("href"))
        .map(str::to_owned)
        .ok_or_else(|| {
            anyhow!("identity response has no redirect link; session was not established")
        })
}
fn trusted_target(url: &str) -> Result<Url> {
    let u = Url::parse(url).or_else(|_| Url::parse(ID)?.join(url))?;
    let host = u.host_str().unwrap_or_default();
    if u.scheme() != "https" || !(host == "tsinghua.edu.cn" || host.ends_with(".tsinghua.edu.cn")) {
        bail!("refusing authentication redirect outside HTTPS tsinghua.edu.cn");
    }
    Ok(u)
}
async fn get(client: &Client, url: &str) -> Result<String> {
    client
        .get(url)
        .send()
        .await
        .map_err(|_| anyhow!("authentication network request failed (URL and response redacted)"))?
        .error_for_status()
        .map_err(|_| anyhow!("authentication HTTP status failed (URL redacted)"))?
        .text()
        .await
        .context("reading authentication response")
}
async fn post(client: &Client, url: &str, form: &[(String, String)]) -> Result<String> {
    trusted_target(url)?;
    client
        .post(url)
        .form(form)
        .send()
        .await
        .map_err(|_| anyhow!("authentication network request failed (URL and response redacted)"))?
        .error_for_status()
        .map_err(|_| anyhow!("authentication HTTP status failed (URL redacted)"))?
        .text()
        .await
        .context("reading authentication response")
}

/// libsm emits 04||C1||C2||C3; sm-crypto expects 04||C1||C3||C2.
pub fn encrypt_password(password: &str, public_key: &str) -> Result<String> {
    if password.is_empty() {
        bail!("password must not be empty");
    }
    let mut bytes = hex::decode(public_key.trim()).context("identity SM2 public key is not hex")?;
    if bytes.len() == 64 {
        bytes.insert(0, 4);
    }
    if bytes.len() != 65 || bytes[0] != 4 {
        bail!("invalid uncompressed SM2 public key");
    }
    let pk = SigCtx::new()
        .load_pubkey(&bytes)
        .map_err(|_| anyhow!("invalid SM2 public key point"))?;
    let raw = EncryptCtx::new(password.len(), pk)
        .encrypt(password.as_bytes())
        .map_err(|_| anyhow!("SM2 encryption failed"))?;
    Ok(hex::encode(
        [&raw[..65], &raw[raw.len() - 32..], &raw[65..raw.len() - 32]].concat(),
    ))
}
fn credential_form(
    options: &LoginOptions,
    public_key: &str,
    website: bool,
    captcha: &str,
) -> Result<Vec<(String, String)>> {
    if options.password.is_empty() {
        return Err(anyhow!(PasswordRequired));
    }
    let mut fields = vec![
        (
            (if website { "username" } else { "i_user" }).into(),
            options.username.clone(),
        ),
        (
            (if website { "password" } else { "i_pass" }).into(),
            encrypt_password(&options.password, public_key)?,
        ),
        ("fingerPrint".into(), options.fingerprint.clone()),
        ("fingerGenPrint".into(), String::new()),
        ("fingerGenPrint3".into(), String::new()),
        ("i_captcha".into(), captcha.into()),
    ];
    if options.trust_device {
        fields.push(("singleLogin".into(), "on".into()));
        fields.push(("deviceName".into(), "thu-cli (macOS)".into()));
    }
    Ok(fields)
}
type IdentityForm = (String, Vec<(String, String)>);
fn passwordless_form(html: &str, fingerprint: &str) -> Result<Option<IdentityForm>> {
    let doc = Html::parse_document(html);
    for form in doc.select(&selector("form")) {
        if form
            .select(&selector("input[name=fingerPrint]"))
            .next()
            .is_none()
        {
            continue;
        }
        let Some(action) = form.value().attr("action") else {
            continue;
        };
        let action = trusted_target(action)?.to_string();
        let fields = form
            .select(&selector("input[name]"))
            .filter_map(|e| {
                let n = e.value().attr("name")?;
                Some((
                    n.to_owned(),
                    if n == "fingerPrint" {
                        fingerprint.to_owned()
                    } else {
                        e.value().attr("value").unwrap_or_default().to_owned()
                    },
                ))
            })
            .collect();
        return Ok(Some((action, fields)));
    }
    Ok(None)
}
async fn identity_response(
    client: &Client,
    entry: &str,
    website: bool,
    options: Option<&LoginOptions>,
) -> Result<String> {
    if entry.contains("登录成功。正在重定向到") {
        return Ok(entry.to_owned());
    }
    let key = text_of(entry, "#sm2publicKey");
    if key.is_empty() {
        let fingerprint = options.map(|o| o.fingerprint.as_str()).unwrap_or_default();
        if let Some((url, fields)) = passwordless_form(entry, fingerprint)? {
            let response = post(client, &url, &fields).await?;
            if !response.contains("sm2publicKey") {
                return Ok(response);
            }
        }
        bail!("identity entry did not provide a public key or usable trusted session; log in again, or use browser session import for unsupported identity challenges");
    }
    let options = options.ok_or_else(|| {
        anyhow!(
            "target system requires credential login; rerun roam with username and password prompt"
        )
    })?;
    let captcha = String::new();
    // Never solve or bypass a challenge. Ask the human when the source explicitly requires it.
    let visible_captcha = entry.contains("验证码不能为空") || entry.contains("验证码错误");
    if visible_captcha {
        if !options.interactive {
            bail!(
                "CAPTCHA required; interactive authentication or browser session import is needed"
            );
        }
        bail!("CAPTCHA image challenge requires browser completion and cookie import; it is not bypassed by this CLI");
    }
    let form = credential_form(options, &key, website, &captcha)?;
    let mut response = if options.service == "learn" && !website {
        // thu-learn-lib uses multipart rather than URL-encoded form data.
        let multipart = form
            .iter()
            .fold(reqwest::multipart::Form::new(), |f, (k, v)| {
                f.text(k.clone(), v.clone())
            });
        client
            .post(CHECK)
            .multipart(multipart)
            .send()
            .await
            .map_err(|_| anyhow!("identity multipart request failed"))?
            .error_for_status()
            .map_err(|_| anyhow!("identity multipart HTTP status failed"))?
            .text()
            .await
            .context("reading identity response")?
    } else {
        post(
            client,
            if website {
                "https://id.tsinghua.edu.cn/security_check"
            } else {
                CHECK
            },
            &form,
        )
        .await?
    };
    if response.contains("二次认证") {
        response = two_factor(client, options).await?;
    }
    Ok(response)
}
fn read_line(prompt: &str) -> Result<String> {
    use std::io::{self, Write};
    eprint!("{prompt}");
    io::stderr().flush()?;
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_owned())
}
async fn two_factor(client: &Client, options: &LoginOptions) -> Result<String> {
    let url = format!("{ID}/b/doubleAuth/login");
    let call = |action: &str| vec![("action".to_owned(), action.to_owned())];
    let methods: Value =
        serde_json::from_str(&post(client, &url, &call("FIND_APPROACHES")).await?)?;
    if methods["result"] != "success" {
        bail!("identity rejected 2FA method discovery");
    }
    if !options.interactive {
        bail!("two-factor authentication required; rerun auth login in an interactive terminal");
    }
    let method = if let Some(m) = &options.two_factor_method {
        m.clone()
    } else {
        eprintln!("Identity requires verification. Available flags: WeChat={}, TOTP={}. SMS availability is determined by your identity account.", methods["object"]["hasWeChatBool"], methods["object"]["hasTotp"]);
        read_line("2FA method (mobile/wechat/totp): ")?
    };
    let method = if method == "sms" {
        "mobile".to_owned()
    } else {
        method
    };
    if !["mobile", "wechat", "totp"].contains(&method.as_str()) {
        bail!("unsupported 2FA method; expected mobile, wechat or totp");
    }
    let sent: Value = serde_json::from_str(
        &post(
            client,
            &url,
            &[
                ("action".into(), "SEND_CODE".into()),
                ("type".into(), method.clone()),
            ],
        )
        .await?,
    )?;
    if sent["result"] != "success" {
        bail!("identity rejected sending verification code");
    }
    let code = rpassword::prompt_password("Verification code (hidden): ")?;
    let verified: Value = serde_json::from_str(
        &post(
            client,
            &url,
            &[
                (
                    "action".into(),
                    if method == "totp" {
                        "VERITY_TOTP_CODE"
                    } else {
                        "VERITY_CODE"
                    }
                    .into(),
                ),
                ("vericode".into(), code),
            ],
        )
        .await?,
    )?;
    if verified["result"] != "success" {
        bail!("identity rejected verification code");
    }
    if options.trust_device {
        let saved: Value = serde_json::from_str(
            &post(
                client,
                SAVE_FINGER,
                &[
                    ("fingerprint".into(), options.fingerprint.clone()),
                    ("deviceName".into(), "Mac mini (thu-cli)".into()),
                    ("radioVal".into(), "是".into()),
                ],
            )
            .await?,
        )?;
        if saved["result"] != "success" {
            eprintln!("Warning: identity did not register this device as trusted");
        } else {
            eprintln!("Identity registered this device as trusted.");
        }
    }
    let redirect = verified["object"]["redirectUrl"]
        .as_str()
        .ok_or_else(|| anyhow!("2FA success lacks redirect"))?;
    get(client, trusted_target(redirect)?.as_str()).await
}
fn check_success(response: &str) -> Result<()> {
    if !response.contains("登录成功。正在重定向到") {
        let note = text_of(response, "#msg_note");
        if note.contains("验证码") || response.contains("captcha") {
            bail!("CAPTCHA or identity challenge required; complete it in browser and import session cookies");
        }
        // Server pages may include input credentials; do not echo untrusted response bodies.
        bail!("identity did not confirm login success; check credentials, account access and any CAPTCHA/MFA requirements");
    }
    Ok(())
}
pub async fn login(client: &Client, options: &LoginOptions) -> Result<Value> {
    if options.username.is_empty() || !options.username.chars().all(|c| c.is_ascii_digit()) {
        bail!("username must be your numeric student/staff ID");
    }
    if options.fingerprint.is_empty() {
        bail!("a stable local device fingerprint is required");
    }
    match options.service.as_str() {
        "info" => {
            if !options.use_webvpn {
                let entry = get(
                    client,
                    &format!("{ID}/do/off/ui/auth/login/form/{INFO_APP}"),
                )
                .await?;
                let response = identity_response(client, &entry, false, Some(options)).await?;
                check_success(&response)?;
                let target = trusted_target(&anchor(&response)?)?;
                get(client, target.as_str()).await?;
                let home = get(client, INFO_HOME).await?;
                if home.contains("sm2publicKey") {
                    bail!("direct information-portal login did not establish a session");
                }
                return Ok(json!({
                    "authenticated": true,
                    "service": options.service,
                    "username": options.username,
                    "network": "campus-direct",
                    "trusted_device_registered": options.trust_device,
                    "verification": "direct information-portal session; no service mutation performed"
                }));
            }
            let entry = get(
                client,
                "https://webvpn.tsinghua.edu.cn/login?oauth_login=true",
            )
            .await?;
            let response = identity_response(client, &entry, false, Some(options)).await?;
            check_success(&response)?;
            get(client, trusted_target(&anchor(&response)?)?.as_str()).await?;
            roam_identity(client, "id", INFO_APP, Some(options)).await?;
            let cookie = get(client, INFO_COOKIE).await?;
            if !cookie.contains("XSRF-TOKEN=") {
                bail!(
                    "WebVPN identity login completed, but information-portal CSRF bootstrap failed"
                );
            }
        }
        "learn" => {
            let entry = get(
                client,
                &format!("{ID}/do/off/ui/auth/login/form/{LEARN_APP}"),
            )
            .await?;
            let response = identity_response(client, &entry, false, Some(options)).await?;
            check_success(&response)?;
            let target = trusted_target(&anchor(&response)?)?;
            let ticket = target
                .query_pairs()
                .find(|(k, _)| k == "ticket")
                .map(|(_, v)| v.into_owned())
                .ok_or_else(|| {
                    anyhow!("identity success did not contain learning-platform ticket")
                })?;
            let mut url = Url::parse(
                "https://learn.tsinghua.edu.cn/b/j_spring_security_thauth_roaming_entry",
            )?;
            url.query_pairs_mut().append_pair("ticket", &ticket);
            get(client, url.as_str()).await?;
            let home = get(
                client,
                "https://learn.tsinghua.edu.cn/f/wlxt/index/course/student/",
            )
            .await?;
            if !home.contains("_csrf=") {
                bail!("learning session did not expose a CSRF token; login is not verified");
            }
        }
        "id" => {
            roam_identity(client, "id_website", "", Some(options)).await?;
        }
        _ => bail!("unknown auth service; expected info, learn or id"),
    }
    Ok(
        json!({"authenticated":true,"service":options.service,"username":options.username,"trusted_device_registered":false,"verification":"target-session bootstrap; no service mutation performed"}),
    )
}
fn webvpn_redirect(target: &str) -> Result<String> {
    let original = Url::parse(target)?;
    let host = original
        .host_str()
        .ok_or_else(|| anyhow!("redirect has no host"))?;
    if !(host == "tsinghua.edu.cn" || host.ends_with(".tsinghua.edu.cn")) {
        bail!("unexpected identity callback host");
    }
    if host == "oauth.tsinghua.edu.cn" {
        return Ok(target.into());
    }
    let mut out = Url::parse("https://oauth.tsinghua.edu.cn/lb-auth/lbredirect")?;
    let mut uri = original.path().to_owned();
    if let Some(q) = original.query() {
        uri.push('?');
        uri.push_str(q);
    }
    if let Some(f) = original.fragment() {
        uri.push('#');
        uri.push_str(f);
    }
    out.query_pairs_mut()
        .append_pair("scheme", original.scheme())
        .append_pair("host", host)
        .append_pair(
            "port",
            &original.port_or_known_default().unwrap_or(443).to_string(),
        )
        .append_pair("uri", &uri);
    Ok(out.to_string())
}
async fn roam_identity(
    client: &Client,
    policy: &str,
    payload: &str,
    credentials: Option<&LoginOptions>,
) -> Result<String> {
    let website = policy == "id_website";
    let entry_url = if website {
        format!("{ID}/f/login")
    } else if policy == "cr" {
        "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421eaff4b8b3f3b2653770bc7b88b5c2d320506b1aec738590a49ba/xklogin.do".into()
    } else {
        format!("{ID}/do/off/ui/auth/login/form/{payload}")
    };
    let entry = get(client, &entry_url).await?;
    if website && text_of(&entry, "title").contains("账号设置") {
        return Ok(entry);
    }
    let response = identity_response(client, &entry, website, credentials).await?;
    if website {
        if !response.contains("账号设置") {
            bail!("account-settings session not established");
        }
        return Ok(response);
    }
    check_success(&response)?;
    let target = anchor(&response)?;
    let target = if policy == "card" {
        trusted_target(&target)?.to_string()
    } else {
        webvpn_redirect(&target)?
    };
    get(client, &target).await
}
pub async fn roam(client: &Client, options: &RoamOptions) -> Result<String> {
    match options.policy.as_str() {
        "cab" if options.payload.is_empty() => {
            cab_login(client, options.credentials.as_ref()).await
        }
        "id" | "id_website" | "card" | "cab" | "cr" => {
            roam_identity(
                client,
                &options.policy,
                &options.payload,
                options.credentials.as_ref(),
            )
            .await
        }
        "gitlab" => {
            let entry = get(client, "https://git.tsinghua.edu.cn/users/sign_in").await?;
            if entry.contains("sign_out") {
                return Ok(entry);
            }
            let doc = Html::parse_document(&entry);
            let token = doc
                .select(&selector("[name=authenticity_token]"))
                .next()
                .and_then(|e| e.value().attr("value"))
                .ok_or_else(|| anyhow!("GitLab authenticity token missing"))?
                .to_owned();
            let entry = post(
                client,
                "https://git.tsinghua.edu.cn/users/auth/thuid",
                &[("authenticity_token".into(), token)],
            )
            .await?;
            let response =
                identity_response(client, &entry, false, options.credentials.as_ref()).await?;
            check_success(&response)?;
            get(client, trusted_target(&anchor(&response)?)?.as_str()).await
        }
        "default" => default_roam(client, &options.payload).await,
        _ => bail!("unknown roaming policy"),
    }
}
async fn cab_login(client: &Client, credentials: Option<&LoginOptions>) -> Result<String> {
    const ROOT: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f3f643d22b396a1e6a1b80a29f5d363409e413829737d1";
    let address: Value = serde_json::from_str(&get(client,&format!("{ROOT}/ic-web/auth/address?finalAddress=https:%2F%2Fcab.lib.tsinghua.edu.cn&errPageUrl=https:%2F%2Fcab.lib.tsinghua.edu.cn%2F%23%2Ferror&manager=false&consoleType=16")).await?)?;
    if address["code"] != 0 {
        bail!("library room system refused auth address request");
    }
    let address = address["data"]
        .as_str()
        .ok_or_else(|| anyhow!("library auth address missing"))?
        .replace("https://cab.lib.tsinghua.edu.cn", ROOT);
    trusted_target(&address)?;
    let response = client
        .get(&address)
        .send()
        .await
        .map_err(|_| anyhow!("library auth redirect request failed"))?
        .error_for_status()
        .map_err(|_| anyhow!("library auth redirect HTTP status failed"))?;
    let destination = response.url().clone();
    let entry = response
        .text()
        .await
        .context("reading library identity entry")?;
    if destination.path().contains("/login/form/") {
        let login_response = identity_response(client, &entry, false, credentials).await?;
        check_success(&login_response)?;
        get(client, &webvpn_redirect(&anchor(&login_response)?)?).await?;
    }
    let user = get(client, &format!("{ROOT}/ic-web/auth/userInfo")).await?;
    let data: Value = serde_json::from_str(&user)?;
    if data["code"] != 0 || data["data"]["accNo"].is_null() {
        bail!("library room session was not established");
    }
    if let Some(c) = credentials {
        if data["data"]["pid"].as_str() != Some(&c.username) {
            bail!("library session belongs to a different identity");
        }
    }
    Ok(user)
}
async fn default_roam(client: &Client, payload: &str) -> Result<String> {
    let cookies = get(client, INFO_COOKIE).await?;
    let csrf = Regex::new(r"XSRF-TOKEN=([^;]+)")?
        .captures(&cookies)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_owned())
        .ok_or_else(|| {
            anyhow!("information-portal CSRF cookie missing; auth login --service info first")
        })?;
    let mut url = Url::parse(INFO_ROAM)?;
    url.query_pairs_mut()
        .append_pair("yyfwid", payload)
        .append_pair("_csrf", &csrf)
        .append_pair("machine", "p");
    let data: Value = serde_json::from_str(&get(client, url.as_str()).await?)?;
    let target = data["object"]["roamingurl"]
        .as_str()
        .ok_or_else(|| anyhow!("portal response contains no roaming URL"))?
        .replace("&amp;", "&");
    let target = legacy_webvpn_url(&target)?;
    if target.contains("f4ed519669247b59700f81b9991b2631aee63c51") {
        let html = get(client, &target).await?;
        let ticket = Regex::new(r#"\("ticket"\).value = '([^']+)';"#)?
            .captures(&html)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_owned())
            .ok_or_else(|| anyhow!("invoice roam ticket missing"))?;
        return post(client,"https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f4ed519669247b59700f81b9991b2631aee63c51/roam/roamAuth.do",&[("ticket".into(),ticket)]).await;
    }
    if target.contains("fdf6459128346d5c300b9ae28c462a3b27469fc32211fa26a3e464") {
        let parsed = Url::parse(&target)?;
        let ticket = parsed
            .query_pairs()
            .find(|(k, _)| k == "ticket")
            .map(|(_, v)| v.into_owned())
            .ok_or_else(|| anyhow!("madmodel roaming ticket missing"))?;
        get(client, &target).await?;
        let mut check=Url::parse("https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421fdf6459128346d5c300b9ae28c462a3b27469fc32211fa26a3e464/model-api/auth-login/check")?;
        check.query_pairs_mut().append_pair("ticket", &ticket);
        return get(client, check.as_str()).await;
    }
    get(client, &target).await
}
fn legacy_webvpn_url(target: &str) -> Result<String> {
    let url = Url::parse(target)?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("roaming target has no host"))?;
    let key = host.strip_suffix(".tsinghua.edu.cn").unwrap_or(host);
    let encrypted=match key {
        "zhjw.cic"=>"77726476706e69737468656265737421eaff4b8b69336153301c9aa596522b20bc86e6e559a9b290",
        "jxgl.cic"|"zhjwxk.cic"=>"77726476706e69737468656265737421faef469069336153301c9aa596522b20e33c1eb39606919f",
        "ecard"=>"77726476706e69737468656265737421f5f4408e237e7c4377068ea48d546d303341e9882a",
        "learn"=>"77726476706e69737468656265737421fcf2408e297e7c4377068ea48d546d30ca8cc97bcc",
        "mails"=>"77726476706e69737468656265737421fdf64890347e7c4377068ea48d546d3011ff591d40",
        "50"=>"77726476706e69737468656265737421a5a70f8834396657761d88e29d51367b6a00",
        "166.111.14.8"=>"77726476706e69737468656265737421a1a117d27661391e2f5cc7f4",
        "fa-online"=>"77726476706e69737468656265737421f6f60c93293c615e7b469dbf915b243daf0f96e17deaf447b4",
        "dzpj"=>"77726476706e69737468656265737421f4ed519669247b59700f81b9991b2631aee63c51",
        "jjhyhdf"=>"77726476706e69737468656265737421fafd49852f346e1e6a1b80a29f5d36342bb9c40cf69277",
        "yhdf"=>"77726476706e69737468656265737421e9ff459a69247b59700f81b9991b26317dbd36ae",
        "usereg"=>"77726476706e69737468656265737421e5e4448e223726446d0187ab9040227b54b6c80fcd73",
        "thos"=>"77726476706e69737468656265737421e4ff4e8f69247b59700f81b9991b2631ca359dd4",
        "zzjl.graduate"=>"77726476706e69737468656265737421eaed4b9069377a517a1d88b89d1b37269c624d2b1c6925f37faea82b8d",
        "madmodel.cs"=>"77726476706e69737468656265737421fdf6459128346d5c300b9ae28c462a3b27469fc32211fa26a3e464",
        _=>bail!("upstream WebVPN host mapping does not include this roaming target"),
    };
    let scheme = if let Some(p) = url.port() {
        format!("{}-{p}", url.scheme())
    } else {
        url.scheme().into()
    };
    let mut out = format!(
        "https://webvpn.tsinghua.edu.cn/{scheme}/{encrypted}{}",
        url.path()
    );
    if let Some(q) = url.query() {
        out.push('?');
        out.push_str(q);
    }
    Ok(out)
}

/// Account security API requests. The CLI catalog must mark writes as mutating;
/// no request is made until the user explicitly executes a confirmed command.
pub async fn manage(client: &Client, operation: &str, args: &Value) -> Result<Value> {
    fn required<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
        args[key]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| anyhow!("missing nonempty string argument: {key}"))
    }
    async fn device_post(
        client: &Client,
        path: &str,
        fields: &[(String, String)],
    ) -> Result<Value> {
        let value: Value =
            serde_json::from_str(&post(client, &format!("{ID}{path}"), fields).await?)?;
        if value["result"] != "success" {
            bail!("identity device-management request was rejected");
        }
        Ok(value["object"].clone())
    }
    match operation {
        "passkey.list" => {
            let result: Value = serde_json::from_str(
                &get(client, &format!("{ID}/api/webauthn/enrollment")).await?,
            )?;
            let devices = result["devices"]
                .as_array()
                .filter(|_| result["ok"] == true)
                .ok_or_else(|| anyhow!("passkey session missing or invalid enrollment list"))?;
            if devices.iter().any(|d| !d["credentialId"].is_string()) {
                bail!("passkey list contains invalid credential ID");
            }
            Ok(Value::Array(devices.clone()))
        }
        "passkey.remove" | "passkey.rename" => {
            let credential = required(args, "credential_id")?;
            let mut url = Url::parse(&format!("{ID}/api/webauthn/enrollment/"))?;
            url.path_segments_mut()
                .map_err(|_| anyhow!("invalid enrollment URL"))?
                .pop_if_empty()
                .push(credential);
            let request = if operation == "passkey.remove" {
                client.delete(url)
            } else {
                client
                    .put(url)
                    .json(&json!({"name":required(args,"name")?.trim()}))
            };
            let result:Value=request.send().await.map_err(|_|anyhow!("passkey request failed; state may have changed, inspect passkey list before retry"))?.error_for_status().map_err(|_|anyhow!("passkey request HTTP status failed; inspect passkey list before retry"))?.json().await.context("invalid passkey response")?;
            if result["ok"] != true {
                bail!("passkey management was rejected");
            }
            Ok(json!({"ok":true,"operation":operation,"credential_id":credential}))
        }
        "device.list" => device_post(client, "/b/account/getDeviceList", &[]).await,
        "device.remove" => {
            device_post(
                client,
                "/b/account/deleteDevice",
                &[("uuid".into(), required(args, "uuid")?.into())],
            )
            .await?;
            Ok(json!({"ok":true,"removed_uuid":required(args,"uuid")?}))
        }
        "device.forget-thu-info" => {
            let fingerprint = required(args, "fingerprint")?;
            let mut removed = Vec::new();
            for _ in 0..10 {
                let mut check = Url::parse(&format!("{ID}/b/account/checkCurrentDeviceIfNeedAdd"))?;
                check
                    .query_pairs_mut()
                    .append_pair("currentFinger", fingerprint)
                    .append_pair("device_gen_finger_print", "null");
                let result: Value =
                    serde_json::from_str(&post(client, check.as_str(), &[]).await?)?;
                if result["result"] != "success" {
                    bail!("device fingerprint check rejected; earlier removals may have succeeded");
                }
                if result["object"] == false {
                    return Ok(
                        json!({"ok":true,"removed_uuids":removed,"fingerprint_condition_resolved":true}),
                    );
                }
                let list = device_post(client, "/b/account/getDeviceList", &[]).await?;
                let device=list.as_array().and_then(|a|a.iter().rev().find(|d|d["name"].as_str().is_some_and(|n|n.starts_with("THU Info APP")))).ok_or_else(||anyhow!("no matching THU Info APP trusted device; earlier removals may have succeeded"))?;
                let id = device["id"]
                    .as_str()
                    .ok_or_else(|| anyhow!("device ID missing"))?
                    .to_owned();
                device_post(
                    client,
                    "/b/account/deleteDevice",
                    &[("uuid".into(), id.clone())],
                )
                .await?;
                removed.push(id);
            }
            Ok(
                json!({"ok":false,"removed_uuids":removed,"fingerprint_condition_resolved":false,"note":"Stopped at upstream 10-removal bound; inspect devices before any retry"}),
            )
        }
        _ => bail!("unknown account-management operation"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libsm::sm2::encrypt::DecryptCtx;
    #[test]
    fn sm2_matches_smcrypto_wire_layout() {
        let ctx = SigCtx::new();
        let (pk, sk) = ctx.new_keypair().unwrap();
        let pubkey = hex::encode(ctx.serialize_pubkey(&pk, false).unwrap());
        let message = "test-password-密码";
        let wire = hex::decode(encrypt_password(message, &pubkey).unwrap()).unwrap();
        assert_eq!(wire[0], 4);
        assert_eq!(wire.len(), 97 + message.len());
        let native = [&wire[..65], &wire[97..], &wire[65..97]].concat();
        assert_eq!(
            DecryptCtx::new(message.len(), sk).decrypt(&native).unwrap(),
            message.as_bytes()
        );
        assert!(encrypt_password("", &pubkey).is_err());
    }
    #[test]
    fn rejects_non_campus_auth_targets() {
        assert!(trusted_target("https://attacker.invalid/check").is_err());
        assert!(trusted_target("http://id.tsinghua.edu.cn/check").is_err());
        assert!(trusted_target("https://eviltsinghua.edu.cn/check").is_err());
    }
    #[test]
    fn parses_trusted_form() {
        let html = r#"<form action='/security_check'><input name='fingerPrint' value=''><input name='i_rememberme' value='on'></form>"#;
        let (url, f) = passwordless_form(html, "local-device").unwrap().unwrap();
        assert_eq!(url, "https://id.tsinghua.edu.cn/security_check");
        assert_eq!(f[0].1, "local-device");
    }
    #[test]
    fn empty_password_is_reported_as_a_credential_request() {
        let options = LoginOptions {
            username: "2023000000".into(),
            password: String::new(),
            fingerprint: "fixture".into(),
            service: "info".into(),
            two_factor_method: None,
            interactive: true,
            trust_device: false,
            use_webvpn: false,
        };
        let error = credential_form(&options, "fixture", false, "").unwrap_err();
        assert!(is_password_required(&error));
    }
    #[test]
    fn trusted_login_requests_a_persistent_browser_session() {
        let ctx = SigCtx::new();
        let (pk, _) = ctx.new_keypair().unwrap();
        let pubkey = hex::encode(ctx.serialize_pubkey(&pk, false).unwrap());
        let options = LoginOptions {
            username: "2023000000".into(),
            password: "fixture-password".into(),
            fingerprint: "fixture".into(),
            service: "info".into(),
            two_factor_method: None,
            interactive: true,
            trust_device: true,
            use_webvpn: false,
        };
        let fields = credential_form(&options, &pubkey, false, "").unwrap();
        assert!(fields
            .iter()
            .any(|(name, value)| name == "singleLogin" && value == "on"));
        assert!(fields
            .iter()
            .any(|(name, value)| name == "deviceName" && !value.is_empty()));
    }
    #[test]
    fn webvpn_encodes_nested_query() {
        let out = webvpn_redirect("https://info.tsinghua.edu.cn/cb?ticket=abc&x=2").unwrap();
        let url = Url::parse(&out).unwrap();
        assert_eq!(
            url.query_pairs().find(|(k, _)| k == "uri").unwrap().1,
            "/cb?ticket=abc&x=2"
        );
    }
    #[test]
    fn legacy_proxy_mapping() {
        assert!(legacy_webvpn_url("http://166.111.14.8:8080/path?a=1")
            .unwrap()
            .contains("/http-8080/"));
        assert!(legacy_webvpn_url("https://example.org/x").is_err());
    }
}
