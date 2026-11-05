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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReimbursementRoute {
    Daily,
    Smart,
    DomesticTravel,
    Conference,
    InternationalTravel,
}

impl ReimbursementRoute {
    fn from_ywlx(ywlx: &str) -> Result<Self> {
        match ywlx {
            "052" => Ok(Self::Daily),
            "051" => Ok(Self::Smart),
            "06" => Ok(Self::DomesticTravel),
            "19" => Ok(Self::Conference),
            "1206" => Ok(Self::InternationalTravel),
            _ => bail!(
                "unsupported reimbursement ywlx {ywlx}; supported values are 051, 052, 06, 19, and 1206"
            ),
        }
    }

    fn ywlx(self) -> &'static str {
        match self {
            Self::Daily => "052",
            Self::Smart => "051",
            Self::DomesticTravel => "06",
            Self::Conference => "19",
            Self::InternationalTravel => "1206",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Smart => "smart-or-foreign-invoice",
            Self::DomesticTravel => "domestic-travel",
            Self::Conference => "conference",
            Self::InternationalTravel => "international-travel",
        }
    }

    fn list_route(self) -> &'static str {
        match self {
            Self::Daily => "zdhb",
            Self::Smart => "rcbx",
            Self::DomesticTravel => "gnclfbx",
            Self::Conference => "hyfbx",
            Self::InternationalTravel => "gjclxhbx",
        }
    }

    fn save_path(self) -> &'static str {
        match self {
            Self::Daily | Self::Smart | Self::Conference => "/pay/whole/submit",
            Self::DomesticTravel => "/clf/gn/tmp/save",
            Self::InternationalTravel => "/intl/travel/save",
        }
    }

    fn template_path(self) -> Option<&'static str> {
        match self {
            Self::Daily => Some("/zdhb/template/get"),
            Self::Smart | Self::Conference => Some("/common/templateInfo/get"),
            Self::DomesticTravel => Some("/common/tempInfo"),
            Self::InternationalTravel => None,
        }
    }
}

fn ywlx_arg(a: &Value) -> Result<&str> {
    a.get("ywlx")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map_or(Ok(DEFAULT_YWLX), Ok)
}

fn route_arg(a: &Value) -> Result<ReimbursementRoute> {
    ReimbursementRoute::from_ywlx(ywlx_arg(a)?)
}

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

fn flag_string(a: &Value, key: &str) -> &'static str {
    match a.get(key) {
        Some(Value::Bool(true)) => "1",
        Some(Value::String(value)) if value == "1" => "1",
        _ => "0",
    }
}

fn flag_string_default(a: &Value, key: &str, default: &'static str) -> Result<&'static str> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Bool(true)) => Ok("1"),
        Some(Value::Bool(false)) => Ok("0"),
        Some(Value::String(value)) if value == "0" || value == "1" => {
            Ok(if value == "1" { "1" } else { "0" })
        }
        Some(_) => bail!("{key} must be a boolean or the string \"0\" or \"1\""),
    }
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

fn invoice_wallet_status(a: &Value) -> Result<&'static str> {
    match a.get("status").and_then(Value::as_str).unwrap_or("pending") {
        "pending" | "unused" | "waiting" | "1" => Ok("1"),
        "used" | "reimbursed" | "history" | "completed" | "0" => Ok("0"),
        _ => bail!("status must be pending, unused, waiting, used, reimbursed, history, completed, 1, or 0"),
    }
}

fn invoice_wallet_search(
    user_id: &str,
    status: &str,
    query: &str,
    page_size: u64,
    page_number: u64,
) -> Value {
    json!({
        "bqids": [],
        "fpje": "",
        "dfdw": "",
        "fkdwmc": "",
        "kpnr": "",
        "dzfph": "",
        "zmr": "",
        "jsr": "",
        "sqrbh": "",
        "bsqrbh": "",
        "fpStatus": status,
        "userId": user_id,
        "fpxx": query,
        "pjlxList": [],
        "orderParam": "",
        "bz": "",
        "ywbh": "",
        "yydh": "",
        "fpyjModify": "",
        "sfsqth": "1",
        "pageSize": page_size,
        "pageNumber": page_number,
        "current": page_number,
        "folderId": null,
        "excludeFolderId": null
    })
}

fn invoice_file_path(a: &Value) -> Result<PathBuf> {
    const SUPPORTED_EXTENSIONS: &[&str] = &["pdf", "ofd", "zip", "bmp", "jpg", "jpeg", "png"];

    let value = required(a, "file")?;
    let path = PathBuf::from(value);
    let metadata =
        fs::symlink_metadata(&path).context("Could not inspect the invoice source file")?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("Invoice source must be a regular file, not a symlink")
    }
    if metadata.len() > MAX_UPLOAD_BYTES {
        bail!("Invoice source exceeds the 30 MiB safety limit")
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| anyhow!("Invoice source must have a supported file extension"))?;
    if !SUPPORTED_EXTENSIONS.contains(&extension.as_str()) {
        bail!(
            "Unsupported invoice source extension {extension}; use PDF, OFD, ZIP, BMP, JPG, JPEG, or PNG"
        )
    }
    Ok(path)
}

fn sanitized_ocr_documents(result: &Value) -> Result<Vec<Value>> {
    let documents = result["data"]
        .as_array()
        .ok_or_else(|| anyhow!("Invoice OCR returned no document array"))?;
    documents
        .iter()
        .enumerate()
        .map(|(index, document)| {
            let code = document["code"].as_str().unwrap_or("unknown");
            if code != "normal" {
                let message = document["outMessage"]
                    .as_str()
                    .unwrap_or("invoice OCR failed");
                bail!(
                    "Invoice OCR document {} failed with {code}: {message}",
                    index + 1
                )
            }
            let mut safe = document
                .as_object()
                .cloned()
                .ok_or_else(|| anyhow!("Invoice OCR document must be a JSON object"))?;
            for field in ["cutfile", "base64", "fileBase64", "zipBase64"] {
                safe.remove(field);
            }
            for field in safe.keys().cloned().collect::<Vec<_>>() {
                if field.ends_with("_zxd") {
                    safe.remove(&field);
                }
            }
            Ok(Value::Object(safe))
        })
        .collect()
}

fn invoice_inv_type(ocr_type: &str) -> &str {
    match ocr_type {
        "0" => "9",
        "1" => "8",
        "2" => "4",
        "3" => "5",
        "5" => "1",
        "6" => "0",
        "7" => "6",
        "8" => "11",
        "10" => "7",
        "12" => "10",
        "14" => "0",
        "16" => "16",
        "18" => "12",
        value => value,
    }
}

fn invoice_ocr_verify_data(document: &Value, file: &Path) -> Result<Value> {
    let object = document
        .as_object()
        .ok_or_else(|| anyhow!("Invoice OCR document must be a JSON object"))?;
    let mut result = object.clone();
    for field in [
        "code",
        "outMessage",
        "cutfile",
        "base64",
        "fileBase64",
        "zipBase64",
    ] {
        result.remove(field);
    }
    for field in result.keys().cloned().collect::<Vec<_>>() {
        if field.ends_with("_zxd") {
            result.remove(&field);
        }
    }

    let ocr_type = object
        .get("fplx")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let inv_type = invoice_inv_type(ocr_type);
    let file_name = file
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("Invoice source filename must be valid UTF-8"))?;
    let file_type = file
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| anyhow!("Invoice source must have a supported file extension"))?;
    let date = object
        .get("kprq")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>();
    let verification_code = object
        .get("fpjym")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .rev()
        .take(6)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    let fpje = object
        .get("fpje")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let sqje = object
        .get("sqje")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let amount = if inv_type == "50" {
        let fpje_number = fpje.parse::<f64>().unwrap_or_default();
        let sqje_number = sqje.parse::<f64>().unwrap_or_default();
        if sqje_number > fpje_number {
            sqje
        } else {
            fpje
        }
    } else {
        fpje
    };

    let empty = json!("");
    let field = |name: &str| object.get(name).cloned().unwrap_or_else(|| empty.clone());
    for key in [
        "fpdm",
        "fphm",
        "kprq",
        "jym",
        "yzm",
        "pickey2",
        "pickey3",
        "isfpyzm",
        "zje",
        "zje_tax",
        "fplx",
        "funcClassify",
        "invType",
        "travellerName",
        "departTime",
        "arriveTime",
        "departSite",
        "arriveSite",
        "vehicleNum",
        "pb",
        "kpnr",
        "kpdwmc",
        "fwlb",
        "fileName",
        "ywlx",
        "invSource",
        "fpfjuuid",
        "cph",
        "zjh",
        "scsj",
        "xcsj",
        "pdfFilePath",
        "zipFilePath",
        "xsfmc",
        "lb",
        "lbmc",
        "route",
        "gmfmc",
        "sj",
        "sfsmj",
    ] {
        result.insert(key.into(), field(key));
    }
    result.insert("kprq".into(), json!(date));
    result.insert("jym".into(), json!(verification_code));
    result.insert("fplx".into(), json!(ocr_type));
    result.insert("invType".into(), json!(inv_type));
    result.insert("zje".into(), json!(amount));
    result.insert("zje_tax".into(), field("fpjetax"));
    result.insert("fileName".into(), json!(file_name));
    result.insert("streamFileName".into(), json!(file_name));
    result.insert("fileType".into(), json!(file_type));
    result.insert("ywlx".into(), json!("113"));
    result.insert("invSource".into(), json!("0"));
    Ok(Value::Object(result))
}

fn upload_content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("pdf") => "application/pdf",
        Some("zip") => "application/zip",
        Some("bmp") => "image/bmp",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("ofd") => "application/ofd",
        _ => "application/octet-stream",
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

fn encrypted_upload_fields(data: &Value) -> Result<(Value, Zeroizing<String>)> {
    encrypted_fields(&json!({"data": data}))
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

fn sum_record_amounts(records: &[Value], key: &str) -> Result<u64> {
    records.iter().try_fold(0_u64, |total, record| {
        total
            .checked_add(amount_cents(record.get(key).unwrap_or(&Value::Null))?)
            .ok_or_else(|| anyhow!("reimbursement amount is too large"))
    })
}

fn daily_amount_totals(payload: &Value) -> Result<(u64, u64)> {
    let items = payload["bxsxList"]
        .as_array()
        .ok_or_else(|| anyhow!("daily payload must contain a bxsxList array"))?;
    let reimbursement = sum_record_amounts(items, "sbje")?;
    let mut payment = 0_u64;
    for key in ["cjkList", "gwkList", "xmzzList", "dszzList", "dgzzList"] {
        let records = payload[key]
            .as_array()
            .ok_or_else(|| anyhow!("daily payload must contain {key} array"))?;
        payment = payment
            .checked_add(sum_record_amounts(records, "dje")?)
            .ok_or_else(|| anyhow!("reimbursement payment amount is too large"))?;
    }
    Ok((reimbursement, payment))
}

fn validate_daily_payment_allocation(payload: &Value) -> Result<()> {
    let (reimbursement, payment) = daily_amount_totals(payload)?;
    if payment == 0 {
        bail!(
            "daily submission requires an allocated payment row; use self_payment:true or provide payments.dszzList"
        )
    }
    if payment != reimbursement {
        bail!(
            "daily payment rows total {} but reimbursement items total {}",
            format_amount(payment),
            format_amount(reimbursement)
        )
    }
    Ok(())
}

fn self_payment_config(value: &Value) -> Result<Option<(Option<String>, String)>> {
    match value {
        Value::Bool(false) | Value::Null => Ok(None),
        Value::Bool(true) => Ok(Some((None, "报销款".to_owned()))),
        Value::Object(object) => {
            let account = object
                .get("account")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            let note = object
                .get("note")
                .and_then(Value::as_str)
                .unwrap_or("报销款")
                .to_owned();
            Ok(Some((account, note)))
        }
        _ => bail!("self_payment must be a boolean or a JSON object"),
    }
}

fn employee_self_payment_row(
    employee: &Value,
    account_hint: Option<&str>,
    amount: &str,
    note: &str,
) -> Result<Value> {
    let ygbh = nonempty_string(employee, "ygbh")?;
    let ygmc = nonempty_string(employee, "ygmc")?;
    let accounts = [
        ("yhzh", "工资卡号", "yhzh", "bank"),
        ("gzkh1", "银行账户1", "gzkh1", "yh1"),
        ("gzkh2", "银行账户2", "gzkh2", "yh2"),
        ("gzkh3", "银行账户3", "gzkh3", "yh3"),
        ("gzkh4", "银行账户4", "gzkh4", "yh4"),
        ("gzkh5", "银行账户5", "gzkh5", "yh5"),
        ("gwkh", "公务卡", "gwkh", "bank"),
    ];
    let available = accounts
        .iter()
        .filter_map(|(kind, label, account_key, bank_key)| {
            employee
                .get(*account_key)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(|account| (*kind, *label, account, *bank_key))
        })
        .collect::<Vec<_>>();

    let (_, label, account, bank_key) = match account_hint {
        Some(hint) => available
            .iter()
            .find(|(kind, _, _, _)| *kind == hint)
            .copied()
            .ok_or_else(|| anyhow!("self_payment.account is not an available bank account"))?,
        None if available.len() == 1 => available[0],
        None if available.is_empty() => {
            bail!("no bank account is available for a private reimbursement payment")
        }
        None => {
            let choices = available
                .iter()
                .map(|(kind, _, _, _)| *kind)
                .collect::<Vec<_>>()
                .join(", ");
            bail!("multiple bank accounts are available; set self_payment.account to one of: {choices}")
        }
    };
    let bank = employee
        .get(bank_key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(label);
    Ok(json!({
        "zffsbh": "08",
        "dfdw": ygbh,
        "wyzgxm": ygmc,
        "yhzh": account,
        "dfzh": account,
        "dfyh": bank,
        "dfpro": "zgbh",
        "dje": amount,
        "fy": note,
        "gwkxfid": ""
    }))
}

fn workflow_flag(value: Option<&Value>) -> Value {
    match value {
        Some(Value::Bool(true)) => json!("1"),
        Some(Value::Bool(false)) | None | Some(Value::Null) => json!("0"),
        Some(Value::Number(number)) if number.as_u64() == Some(1) => json!("1"),
        Some(Value::String(value)) if value == "1" => json!("1"),
        _ => json!("0"),
    }
}

fn normalized_workflow(value: &Value, records: bool) -> Result<Vec<Value>> {
    let rows = value
        .as_array()
        .ok_or_else(|| anyhow!("approval workflow must be a JSON array"))?;
    let mut workflow = rows
        .iter()
        .map(|row| {
            let object = row
                .as_object()
                .ok_or_else(|| anyhow!("approval workflow rows must be JSON objects"))?;
            let mut normalized = if records {
                json!({
                    "rank": object.get("rank").cloned().unwrap_or(Value::Null),
                    "roleId": object.get("roleId").cloned().unwrap_or(Value::Null),
                    "roleNm": object.get("roleNm").cloned().unwrap_or(Value::Null),
                    "ygbh": object.get("ygbh").cloned().unwrap_or(Value::Null),
                    "ygmc": object.get("ygmc").cloned().unwrap_or(Value::Null),
                    "BJLC": object.get("bjlc").cloned().unwrap_or(Value::Null),
                    "TSDX": object.get("tsdx").cloned().unwrap_or(Value::Null),
                    "SFQZ": object.get("sfqz").cloned().unwrap_or(Value::Null),
                    "SFQM": object.get("sfqm").cloned().unwrap_or(Value::Null),
                    "SPFS": object.get("spfs").cloned().unwrap_or(Value::Null)
                })
            } else {
                Value::Object(object.clone())
            };
            normalized["TSDX"] = workflow_flag(normalized.get("TSDX"));
            normalized["BJLC"] = workflow_flag(normalized.get("BJLC"));
            Ok(normalized)
        })
        .collect::<Result<Vec<_>>>()?;
    workflow.sort_by_key(|row| {
        row.get("rank")
            .and_then(|value| {
                value
                    .as_u64()
                    .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
            })
            .unwrap_or(u64::MAX)
    });
    Ok(workflow)
}

fn validate_workflow(workflow: &[Value]) -> Result<()> {
    if workflow.is_empty() {
        bail!("approval workflow is empty")
    }
    let mut levels = Vec::new();
    for (index, row) in workflow.iter().enumerate() {
        let rank = row["rank"]
            .as_u64()
            .or_else(|| row["rank"].as_str().and_then(|text| text.parse().ok()))
            .ok_or_else(|| anyhow!("approval workflow row {} has no valid rank", index + 1))?;
        if index == 0 && rank != 1 {
            bail!("approval workflow must start at rank 1")
        }
        for key in ["ygbh", "ygmc", "roleNm"] {
            if row[key].as_str().is_none_or(str::is_empty) {
                bail!("approval workflow row {} is missing {key}", index + 1)
            }
        }
        levels.push(rank);
    }
    let mut unique = levels.clone();
    unique.sort_unstable();
    unique.dedup();
    for rank in unique {
        if !workflow.iter().any(|row| {
            let row_rank = row["rank"]
                .as_u64()
                .or_else(|| row["rank"].as_str().and_then(|text| text.parse().ok()));
            row_rank == Some(rank) && row["TSDX"] == "1"
        }) {
            bail!("approval rank {rank} must include at least one approver receiving notifications")
        }
    }
    Ok(())
}

async fn approval_workflow(
    client: &mut WsbxClient<'_>,
    ywbh: &str,
    ywlx: &str,
    is_cwzg: bool,
) -> Result<Value> {
    let records = client
        .get("/workflow/record/get", json!({"ywbh": ywbh}), None)
        .await?;
    if records["data"]
        .as_array()
        .is_some_and(|rows| !rows.is_empty())
    {
        let workflow = normalized_workflow(&records["data"], true)?;
        validate_workflow(&workflow)?;
        return Ok(json!({"source": "record", "workflow": workflow}));
    }

    let status = client
        .get("/common/get/ywbxr", json!({"ywbh": ywbh}), None)
        .await?;
    let base = &status["data"];
    let workflow = client
        .post_read(
            "/approve/workflow/get",
            json!({
                "yydh": base["yydh"],
                "bmbh": base["bmbh"],
                "xmbh": base["xmbh"],
                "ywlx": ywlx,
                "isCwzg": is_cwzg,
                "hyfIsCwzg": false
            }),
            None,
        )
        .await?;
    let workflow = normalized_workflow(&workflow["data"], false)?;
    validate_workflow(&workflow)?;
    Ok(json!({"source": "generated", "workflow": workflow}))
}

async fn check_approval_workflow(
    client: &mut WsbxClient<'_>,
    ywbh: &str,
    workflow: &[Value],
    is_cwzg: bool,
) -> Result<Value> {
    validate_workflow(workflow)?;
    let checked = client
        .post_read(
            "/approve/spxxdl/check",
            json!({
                "ywbh": ywbh,
                "workflow": serde_json::to_string(workflow)?,
                "isCwzg": is_cwzg
            }),
            None,
        )
        .await?;
    let code = checked["data"]["code"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| {
            checked["data"]["code"]
                .as_i64()
                .map(|value| value.to_string())
        })
        .unwrap_or_default();
    if code != "0" {
        let message = checked["data"]["msg"]
            .as_str()
            .or_else(|| checked["msg"].as_str())
            .unwrap_or("approval workflow requires review");
        bail!("approval workflow check failed with code {code}: {message}")
    }
    Ok(checked)
}

async fn submit_approval(
    client: &mut WsbxClient<'_>,
    ywbh: &str,
    ywlx: &str,
    args: &Value,
) -> Result<Value> {
    let status = client
        .get("/common/get/ywbxr", json!({"ywbh": ywbh}), None)
        .await?;
    if status["data"]["zt"].as_str() != Some("8") {
        bail!("approval submission requires receipt state 8")
    }
    let is_cwzg = bool_arg(args, "is_cwzg", true);
    let workflow = if let Some(workflow) = args.get("workflow") {
        let workflow = normalized_workflow(workflow, false)?;
        validate_workflow(&workflow)?;
        json!({"source": "provided", "workflow": workflow})
    } else {
        approval_workflow(client, ywbh, ywlx, is_cwzg).await?
    };
    let rows = workflow["workflow"]
        .as_array()
        .ok_or_else(|| anyhow!("approval workflow is invalid"))?;
    check_approval_workflow(client, ywbh, rows, is_cwzg).await?;

    let paperless = client
        .post_read(
            "/common/wzh/check",
            json!({"ywbh": ywbh, "ywlx": ywlx}),
            None,
        )
        .await?;
    let paperless_available = paperless["data"]["paperless"].as_bool() == Some(true)
        || paperless["data"]["paperless"].as_str() == Some("1");
    let wzh_flag = args
        .get("wzh_flag")
        .map(|value| workflow_flag(Some(value)) == "1")
        .unwrap_or(paperless_available);
    let result = client
        .post(
            "/approve/spxxdl/submit",
            json!({
                "ywbh": ywbh,
                "ywbh2": args.get("ywbh2").and_then(Value::as_str).unwrap_or(""),
                "workflow": serde_json::to_string(rows)?,
                "lcid": args.get("lcid").and_then(Value::as_str).unwrap_or(""),
                "lcmc": args.get("lcmc").and_then(Value::as_str).unwrap_or(""),
                "isSaveGxhlc": false,
                "wzhFlag": wzh_flag,
                "bzContent": args.get("note").and_then(Value::as_str).unwrap_or(""),
                "jbrbh": user_id(client)?
            }),
            None,
        )
        .await?;
    Ok(json!({
        "business_number": ywbh,
        "submitted": true,
        "workflow_source": workflow["source"],
        "wzh_flag": wzh_flag,
        "result": result["data"]
    }))
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

fn daily_payload(a: &Value, submit: bool) -> Result<Value> {
    let ywbh = required(a, "ywbh")?;
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
    // Finance's normal domestic e-invoice path is paperless with online push.
    // Callers can still explicitly override either choice with false or "0".
    let is_online = flag_string_default(a, "is_online", "1")?;
    let no_paper = flag_string_default(a, "no_paper", "1")?;
    let state = if !submit {
        "0"
    } else if is_online == "1" {
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
        "isOnline": is_online,
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

fn object_or_empty(a: &Value, key: &str) -> Result<Map<String, Value>> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(Map::new()),
        Some(Value::Object(object)) => Ok(object.clone()),
        Some(_) => bail!("{key} must be a JSON object"),
    }
}

fn array_or_empty(value: &Value, key: &str) -> Result<Vec<Value>> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(values)) => Ok(values.clone()),
        Some(_) => bail!("{key} must be a JSON array"),
    }
}

fn value_or_empty(value: &Value, key: &str) -> Value {
    value.get(key).cloned().unwrap_or_else(|| json!(""))
}

fn first_value(a: &Value, details: &Value, key: &str) -> Value {
    details
        .get(key)
        .or_else(|| a.get(key))
        .cloned()
        .unwrap_or_else(|| json!(""))
}

fn array_value(a: &Value, details: &Value, key: &str) -> Result<Vec<Value>> {
    let value = details.get(key).or_else(|| a.get(key));
    match value {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(values)) => Ok(values.clone()),
        Some(_) => bail!("{key} must be a JSON array"),
    }
}

fn insert_project(base: &mut Map<String, Value>, project: Option<&Value>) {
    let Some(project) = project else {
        return;
    };
    if let Some(bmbh) = project.get("bmbh") {
        base.entry("bmbh".to_owned())
            .or_insert_with(|| bmbh.clone());
    }
    if let Some(xmbh) = project.get("xmbh") {
        base.entry("xmbh".to_owned())
            .or_insert_with(|| xmbh.clone());
    }
    if let Some(bmmc) = project.get("bmmc") {
        base.entry("bmmc".to_owned())
            .or_insert_with(|| bmmc.clone());
    }
    if let Some(xmmc) = project.get("xmmc") {
        base.entry("xmmc".to_owned())
            .or_insert_with(|| xmmc.clone());
    }
}

fn insert_payment_lists(payload: &mut Map<String, Value>, a: &Value) -> Result<()> {
    for (key, value) in payments(a)? {
        payload.insert(key, value);
    }
    Ok(())
}

fn international_pay(a: &Value, submit: bool, operator: &str, ywbh: &str) -> Result<Value> {
    let mut pay = a
        .get("payload")
        .and_then(|payload| payload.get("pay"))
        .and_then(Value::as_object)
        .cloned()
        .map_or_else(|| object_or_empty(a, "pay"), Ok)?;
    for (key, value) in payments(a)? {
        pay.entry(key).or_insert(value);
    }
    let project = a.get("project").or_else(|| a.get("base_info")).or_else(|| {
        a.get("intl_budget")
            .and_then(|budget| budget.get("data"))
            .and_then(|budget| budget.get("base"))
            .or_else(|| {
                a.get("intl")
                    .and_then(|budget| budget.get("data"))
                    .and_then(|budget| budget.get("base"))
            })
    });
    if let Some(project) = project {
        insert_project(&mut pay, Some(project));
    }
    pay.entry("ywbh".to_owned()).or_insert_with(|| json!(ywbh));
    pay.entry("ywlx".to_owned())
        .or_insert_with(|| json!(ReimbursementRoute::InternationalTravel.ywlx()));
    pay.entry("loginUserId".to_owned())
        .or_insert_with(|| json!(operator));
    pay.entry("loginUserName".to_owned())
        .or_insert_with(|| json!(a.get("operator_name").and_then(Value::as_str).unwrap_or("")));
    pay.entry("isOnline".to_owned())
        .or_insert_with(|| json!(flag_string(a, "is_online")));
    pay.insert("saveType".to_owned(), json!(if submit { "1" } else { "0" }));
    pay.entry("cwcs".to_owned()).or_insert_with(|| json!("0"));
    pay.entry("warnList".to_owned())
        .or_insert_with(|| json!([]));
    pay.entry("hasPaidAmount".to_owned())
        .or_insert_with(|| json!("0"));
    Ok(Value::Object(pay))
}

fn smart_payload(a: &Value, submit: bool, operator: &str) -> Result<Value> {
    let ywbh = required(a, "ywbh")?;
    let items = a
        .get("items")
        .map(|value| {
            value
                .as_array()
                .cloned()
                .ok_or_else(|| anyhow!("items must be a JSON array"))
        })
        .transpose()?
        .unwrap_or_default();
    let invoices = a
        .get("invoice_list")
        .map(|value| {
            value
                .as_array()
                .cloned()
                .ok_or_else(|| anyhow!("invoice_list must be a JSON array"))
        })
        .transpose()?
        .unwrap_or_default();
    let paper_items = a
        .get("paper_items")
        .map(|value| {
            value
                .as_array()
                .cloned()
                .ok_or_else(|| anyhow!("paper_items must be a JSON array"))
        })
        .transpose()?
        .unwrap_or_default();
    let base_smart = object_or_empty(a, "base_info")?;
    let bxsy = base_smart
        .get("bxsy")
        .and_then(Value::as_str)
        .or_else(|| a.get("purpose").and_then(Value::as_str))
        .unwrap_or("");
    let bxqksm = base_smart
        .get("bxqksm")
        .and_then(Value::as_str)
        .or_else(|| a.get("description").and_then(Value::as_str))
        .unwrap_or("");
    let mut payload = json!({
        "ywbh": ywbh,
        "saveType": if submit { "1" } else { "0" },
        "invoiceList": invoices,
        "bxmbInfoList": items,
        "ybbxBaseInfo": {
            "bxsy": bxsy,
            "bxqksm": bxqksm
        },
        "lxdh": a.get("phone").and_then(Value::as_str).unwrap_or(""),
        "isOnline": flag_string(a, "is_online"),
        "noPaper": delivery_option(a, "no_paper", submit)?,
        "czr": operator
    })
    .as_object()
    .cloned()
    .ok_or_else(|| anyhow!("smart reimbursement payload is invalid"))?;
    if !paper_items.is_empty() {
        payload.insert("bxmbInfoList".to_owned(), json!(paper_items));
    }
    insert_payment_lists(&mut payload, a)?;
    Ok(Value::Object(payload))
}

fn domestic_travel_payload(a: &Value, submit: bool, operator: &str) -> Result<Value> {
    let ywbh = required(a, "ywbh")?;
    let details = a
        .get("travel")
        .or_else(|| a.get("details"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !details.is_object() {
        bail!("travel must be a JSON object")
    }
    let project = a.get("project");
    let mut ywbxr_base = a
        .get("base_info")
        .or_else(|| details.get("ywbxrBase"))
        .map(|value| {
            value
                .as_object()
                .cloned()
                .ok_or_else(|| anyhow!("base_info must be a JSON object"))
        })
        .transpose()?
        .unwrap_or_default();
    insert_project(&mut ywbxr_base, project);
    ywbxr_base
        .entry("ygbh".to_owned())
        .or_insert_with(|| json!(operator));
    ywbxr_base
        .entry("fzrbh".to_owned())
        .or_insert_with(|| json!(operator));
    if let Some(name) = a.get("operator_name").and_then(Value::as_str) {
        ywbxr_base
            .entry("ygmc".to_owned())
            .or_insert_with(|| json!(name));
    }
    ywbxr_base
        .entry("lxdh".to_owned())
        .or_insert_with(|| json!(a.get("phone").and_then(Value::as_str).unwrap_or("")));

    let mut base_info = details
        .get("baseInfoObj")
        .map(|value| {
            value
                .as_object()
                .cloned()
                .ok_or_else(|| anyhow!("baseInfoObj must be a JSON object"))
        })
        .transpose()?
        .unwrap_or_default();
    insert_project(&mut base_info, project);
    base_info
        .entry("bxsy".to_owned())
        .or_insert_with(|| json!(a.get("purpose").and_then(Value::as_str).unwrap_or("")));
    base_info
        .entry("ccdd".to_owned())
        .or_insert_with(|| json!(details.get("ccdd").cloned().unwrap_or_else(|| json!(""))));
    let mut bxr_info = base_info
        .get("bxrInfo")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    bxr_info
        .entry("bxrbh".to_owned())
        .or_insert_with(|| json!(operator));
    if let Some(name) = a.get("operator_name").and_then(Value::as_str) {
        bxr_info
            .entry("bxrmc".to_owned())
            .or_insert_with(|| json!(name));
    }
    bxr_info
        .entry("lxdh".to_owned())
        .or_insert_with(|| json!(a.get("phone").and_then(Value::as_str).unwrap_or("")));
    base_info.insert("bxrInfo".to_owned(), Value::Object(bxr_info));

    let invoices = array_value(a, &details, "invoiceList")?;
    let paper_items = array_value(a, &details, "fpjInfoList")?;
    let special = array_value(a, &details, "bzList")?;
    let default_traveler_id = first_value(a, &details, "cczmrbh");
    let default_traveler_id = if default_traveler_id.as_str().is_some_and(str::is_empty) {
        json!(operator)
    } else {
        default_traveler_id
    };
    let default_traveler_name = first_value(a, &details, "cczmrmc");
    let default_traveler_name = if default_traveler_name.as_str().is_some_and(str::is_empty) {
        json!(a.get("operator_name").and_then(Value::as_str).unwrap_or(""))
    } else {
        default_traveler_name
    };
    let mut payload = json!({
        "ywbh": ywbh,
        "cllx": value_or_empty(&details, "cllx"),
        "ccsy": first_value(a, &details, "ccsy"),
        "ccrq": value_or_empty(&details, "ccrq"),
        "ts": value_or_empty(&details, "ts"),
        "ccdd": first_value(a, &details, "ccdd"),
        "cczmrbh": default_traveler_id,
        "cczmrmc": default_traveler_name,
        "lrrdh": a.get("phone").and_then(Value::as_str).unwrap_or(""),
        "saveType": if submit { "1" } else { "0" },
        "loginUserId": operator,
        "loginUserName": a.get("operator_name").and_then(Value::as_str).unwrap_or(""),
        "ywlx": ReimbursementRoute::DomesticTravel.ywlx(),
        "bzList": special,
        "czr": operator,
        "fpjInfoList": paper_items,
        "invoiceList": invoices,
        "ywbxrBase": ywbxr_base,
        "baseInfoObj": base_info,
        "isOnline": flag_string(a, "is_online"),
        "noPaper": delivery_option(a, "no_paper", submit)?
    })
    .as_object()
    .cloned()
    .ok_or_else(|| anyhow!("domestic travel payload is invalid"))?;
    insert_payment_lists(&mut payload, a)?;
    Ok(Value::Object(payload))
}

fn conference_payload(a: &Value, submit: bool, operator: &str) -> Result<Value> {
    let ywbh = required(a, "ywbh")?;
    let details = a
        .get("conference")
        .or_else(|| a.get("details"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !details.is_object() {
        bail!("conference must be a JSON object")
    }
    let mut base = a
        .get("conference_base")
        .or_else(|| details.get("hyfBaseInfo"))
        .map(|value| {
            value
                .as_object()
                .cloned()
                .ok_or_else(|| anyhow!("conference_base must be a JSON object"))
        })
        .transpose()?
        .unwrap_or_else(|| {
            let details = details.as_object().cloned().unwrap_or_default();
            details
                .into_iter()
                .filter(|(key, _)| {
                    [
                        "ywbh",
                        "hymc",
                        "hydd",
                        "hylb",
                        "jbdw",
                        "hyxs",
                        "hykssj",
                        "hyjssj",
                        "xschrs",
                        "xxxnrs",
                        "xxxwrs",
                        "gzrs",
                        "ysbh",
                        "ysfl",
                        "jnf",
                        "bdlkts",
                        "bhfs",
                        "xxxnrsold",
                        "xxxwrsold",
                        "isxsys",
                        "zrs",
                        "hyts",
                        "dbrs",
                    ]
                    .contains(&key.as_str())
                })
                .collect()
        });
    base.insert("ywbh".to_owned(), json!(ywbh));

    let expenses = if let Some(values) = a.get("items").and_then(Value::as_array) {
        values.clone()
    } else {
        array_value(a, &details, "hyfZcmxList")?
    };
    let invoices = array_value(a, &details, "invoiceList")?;
    let paper_items = array_value(a, &details, "bxmbInfoList")?;
    let zt = if !submit {
        "0"
    } else if flag_string(a, "is_online") == "1" {
        "8"
    } else {
        "5"
    };
    let mut payload = json!({
        "ywbh": ywbh,
        "saveType": if submit { "1" } else { "0" },
        "invoiceList": invoices,
        "bxmbInfoList": paper_items,
        "lxdh": a.get("phone").and_then(Value::as_str).unwrap_or(""),
        "zt": zt,
        "isOnline": flag_string(a, "is_online"),
        "noPaper": delivery_option(a, "no_paper", submit)?,
        "czr": operator,
        "hyfZcmxList": expenses,
        "hyfBaseInfo": base
    })
    .as_object()
    .cloned()
    .ok_or_else(|| anyhow!("conference payload is invalid"))?;
    insert_payment_lists(&mut payload, a)?;
    Ok(Value::Object(payload))
}

fn international_travel_payload(a: &Value, submit: bool, operator: &str) -> Result<Value> {
    let ywbh = required(a, "ywbh")?;
    let envelope = a
        .get("intl_budget")
        .or_else(|| a.get("intl"))
        .or_else(|| a.get("details"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !envelope.is_object() {
        bail!("intl must be a JSON object")
    }
    let details = envelope.get("data").unwrap_or(&envelope);
    if !details.is_object() {
        bail!("intl.data must be a JSON object")
    }
    let project = a.get("project");
    let mut base = details
        .get("base")
        .or_else(|| a.get("base_info"))
        .map(|value| {
            value
                .as_object()
                .cloned()
                .ok_or_else(|| anyhow!("intl base must be a JSON object"))
        })
        .transpose()?
        .unwrap_or_default();
    insert_project(&mut base, project);
    base.insert("ywbh".to_owned(), json!(ywbh));
    base.insert(
        "ywlx".to_owned(),
        json!(ReimbursementRoute::InternationalTravel.ywlx()),
    );
    if let Some(line) = a.get("actual_line") {
        base.insert("actualLine".to_owned(), line.clone());
    }
    if let Some(start) = a.get("actual_start_date") {
        base.insert("actualStartDate".to_owned(), start.clone());
    }
    if let Some(end) = a.get("actual_end_date") {
        base.insert("actualEndDate".to_owned(), end.clone());
    }

    let travelers = if let Some(values) = a.get("travelers").and_then(Value::as_array) {
        values.clone()
    } else {
        array_or_empty(details, "ry")?
    };
    let expenses = if let Some(values) = a.get("expense_rows").and_then(Value::as_array) {
        values.clone()
    } else {
        array_or_empty(details, "wb")?
    };
    let details_rows = array_or_empty(details, "wbDetail")?;
    let allocations = array_or_empty(details, "ftfy")?;
    let statements = array_or_empty(details, "statement")?;
    let pay = international_pay(a, submit, operator, &ywbh)?;
    Ok(json!({
        "saveType": if submit { "1" } else { "0" },
        "isOnline": flag_string(a, "is_online"),
        "noPaper": delivery_option(a, "no_paper", submit)?,
        "convertInaccuracy": a.get("convert_inaccuracy").cloned().unwrap_or_else(|| json!("0")),
        "base": base,
        "ry": travelers,
        "wb": expenses,
        "wbDetail": details_rows,
        "ftfy": allocations,
        "pay": pay,
        "currencyRate": details.get("currencyRate").cloned().unwrap_or_else(|| json!({})),
        "reexchange": details.get("reexchange").cloned().unwrap_or_else(|| json!({})),
        "conversion": details.get("conversion").cloned().unwrap_or_else(|| json!({})),
        "statement": statements
    }))
}

fn route_payload(
    a: &Value,
    route: ReimbursementRoute,
    submit: bool,
    operator: &str,
) -> Result<Value> {
    match route {
        ReimbursementRoute::Daily => daily_payload(a, submit),
        ReimbursementRoute::Smart => smart_payload(a, submit, operator),
        ReimbursementRoute::DomesticTravel => domestic_travel_payload(a, submit, operator),
        ReimbursementRoute::Conference => conference_payload(a, submit, operator),
        ReimbursementRoute::InternationalTravel => {
            international_travel_payload(a, submit, operator)
        }
    }
}

fn route_payload_from_external(
    a: &Value,
    route: ReimbursementRoute,
    submit: bool,
    operator: &str,
) -> Result<Value> {
    let mut payload = a
        .get("payload")
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| anyhow!("payload must be a JSON object"))?;
    let ywbh = required(a, "ywbh")?;
    if payload.get("ywbh").and_then(Value::as_str) != Some(ywbh.as_str()) {
        bail!("payload.ywbh must exactly match the ywbh argument")
    }
    payload.insert("saveType".to_owned(), json!(if submit { "1" } else { "0" }));
    let no_paper = if route == ReimbursementRoute::Daily {
        flag_string_default(a, "no_paper", "1")?.to_owned()
    } else {
        delivery_option(a, "no_paper", submit)?
    };
    let online = if route == ReimbursementRoute::Daily {
        flag_string_default(a, "is_online", "1")?
    } else {
        flag_string(a, "is_online")
    };
    match route {
        ReimbursementRoute::Daily => {
            payload.entry("zt".to_owned()).or_insert_with(|| {
                json!(if !submit {
                    "0"
                } else if online == "1" {
                    "8"
                } else {
                    "5"
                })
            });
            payload
                .entry("bxsxType".to_owned())
                .or_insert_with(|| json!("zdhb"));
            payload
                .entry("isOnline".to_owned())
                .or_insert_with(|| json!(online));
            payload
                .entry("noPaper".to_owned())
                .or_insert_with(|| json!(no_paper));
            insert_payment_lists(&mut payload, a)?;
        }
        ReimbursementRoute::Smart => {
            payload
                .entry("isOnline".to_owned())
                .or_insert_with(|| json!(online));
            payload
                .entry("noPaper".to_owned())
                .or_insert_with(|| json!(no_paper));
            payload
                .entry("czr".to_owned())
                .or_insert_with(|| json!(operator));
            insert_payment_lists(&mut payload, a)?;
        }
        ReimbursementRoute::DomesticTravel => {
            payload
                .entry("ywlx".to_owned())
                .or_insert_with(|| json!(route.ywlx()));
            payload
                .entry("loginUserId".to_owned())
                .or_insert_with(|| json!(operator));
            payload
                .entry("czr".to_owned())
                .or_insert_with(|| json!(operator));
            insert_payment_lists(&mut payload, a)?;
        }
        ReimbursementRoute::Conference => {
            payload.entry("zt".to_owned()).or_insert_with(|| {
                json!(if !submit {
                    "0"
                } else if online == "1" {
                    "8"
                } else {
                    "5"
                })
            });
            payload
                .entry("isOnline".to_owned())
                .or_insert_with(|| json!(online));
            payload
                .entry("noPaper".to_owned())
                .or_insert_with(|| json!(no_paper));
            payload
                .entry("czr".to_owned())
                .or_insert_with(|| json!(operator));
            if let Some(hyf) = payload
                .get_mut("hyfBaseInfo")
                .and_then(Value::as_object_mut)
            {
                hyf.insert("ywbh".to_owned(), json!(ywbh));
            }
            insert_payment_lists(&mut payload, a)?;
        }
        ReimbursementRoute::InternationalTravel => {
            let pay = international_pay(a, submit, operator, &ywbh)?;
            payload
                .entry("isOnline".to_owned())
                .or_insert_with(|| json!(online));
            payload
                .entry("noPaper".to_owned())
                .or_insert_with(|| json!(no_paper));
            if let Some(base) = payload.get_mut("base").and_then(Value::as_object_mut) {
                base.insert("ywbh".to_owned(), json!(ywbh));
                base.insert("ywlx".to_owned(), json!(route.ywlx()));
                insert_project(base, a.get("project"));
            }
            payload.insert("pay".to_owned(), pay);
        }
    }
    Ok(Value::Object(payload))
}

fn validate_route_payload(route: ReimbursementRoute, payload: &Value, submit: bool) -> Result<()> {
    if !payload.is_object() {
        bail!("reimbursement payload must be a JSON object")
    }
    match route {
        ReimbursementRoute::Daily => {
            if !payload["bxsxList"].is_array() {
                bail!("daily payload must contain a bxsxList array")
            }
            if !payload["ybbxBaseInfo"].is_object() {
                bail!("daily payload must contain a ybbxBaseInfo object")
            }
        }
        ReimbursementRoute::Smart => {
            let invoices = payload["invoiceList"]
                .as_array()
                .ok_or_else(|| anyhow!("smart payload must contain an invoiceList array"))?;
            let items = payload["bxmbInfoList"]
                .as_array()
                .ok_or_else(|| anyhow!("smart payload must contain a bxmbInfoList array"))?;
            if submit && invoices.is_empty() && items.is_empty() {
                bail!("smart reimbursement submission requires an invoice or paper expense item")
            }
        }
        ReimbursementRoute::DomesticTravel => {
            payload["ywbxrBase"]
                .as_object()
                .ok_or_else(|| anyhow!("domestic travel payload requires ywbxrBase"))?;
            payload["baseInfoObj"]
                .as_object()
                .ok_or_else(|| anyhow!("domestic travel payload requires baseInfoObj"))?;
            if submit {
                for key in ["cllx", "ccsy", "ccdd", "ccrq"] {
                    if payload.get(key).is_none_or(Value::is_null) {
                        bail!("domestic travel submission requires {key}")
                    }
                }
            }
        }
        ReimbursementRoute::Conference => {
            payload["hyfBaseInfo"]
                .as_object()
                .ok_or_else(|| anyhow!("conference payload requires hyfBaseInfo"))?;
            let expenses = payload["hyfZcmxList"]
                .as_array()
                .ok_or_else(|| anyhow!("conference payload requires hyfZcmxList"))?;
            if submit && expenses.is_empty() {
                bail!("conference submission requires at least one hyfZcmxList row")
            }
        }
        ReimbursementRoute::InternationalTravel => {
            let base = payload["base"]
                .as_object()
                .ok_or_else(|| anyhow!("international travel payload requires base"))?;
            let travelers = payload["ry"]
                .as_array()
                .ok_or_else(|| anyhow!("international travel payload requires ry"))?;
            let expenses = payload["wb"]
                .as_array()
                .ok_or_else(|| anyhow!("international travel payload requires wb"))?;
            if submit {
                if travelers.is_empty() {
                    bail!("international travel submission requires at least one traveler")
                }
                if expenses.is_empty() {
                    bail!("international travel submission requires at least one expense row")
                }
                for key in ["actualLine", "actualStartDate", "actualEndDate"] {
                    if base
                        .get(key)
                        .and_then(Value::as_str)
                        .is_none_or(str::is_empty)
                    {
                        bail!("international travel submission requires base.{key}")
                    }
                }
            }
        }
    }
    Ok(())
}

fn save_payload(
    a: &Value,
    route: ReimbursementRoute,
    submit: bool,
    operator: &str,
) -> Result<Value> {
    let payload = if a.get("payload").is_some() {
        route_payload_from_external(a, route, submit, operator)?
    } else {
        route_payload(a, route, submit, operator)?
    };
    validate_route_payload(route, &payload, submit)?;
    Ok(payload)
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
            let detail = decrypt_response(&body, &aes_key)
                .ok()
                .and_then(|value| {
                    let code = envelope_code(&value)?;
                    let message = value
                        .get("msg")
                        .and_then(Value::as_str)
                        .unwrap_or("request rejected");
                    Some(format!("{code}: {message}"))
                })
                .or_else(|| {
                    std::str::from_utf8(&body)
                        .ok()
                        .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
                        .filter(|text| !text.is_empty())
                        .map(|text| text.chars().take(800).collect())
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
        let filename = file
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| anyhow!("Upload filename must be valid UTF-8"))?;
        self.upload_as(
            path,
            header,
            file,
            filename,
            upload_content_type(file),
            fields,
        )
        .await
    }

    async fn upload_as(
        &mut self,
        path: &str,
        header: Option<&str>,
        file: &Path,
        part_filename: &str,
        part_content_type: &str,
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
        let bytes = fs::read(file).context("Could not read upload file")?;
        let part = Part::bytes(bytes)
            .file_name(part_filename.to_owned())
            .mime_str(part_content_type)
            .context("Invalid upload content type")?;
        let (encrypted, aes_key) = encrypted_upload_fields(&fields)?;
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
            .header("Origin", "https://wsbx.tsinghua.edu.cn")
            .header("Referer", "https://wsbx.tsinghua.edu.cn/wsyy/fpgl")
            .multipart(Form::new().part("file", part))
            .timeout(Duration::from_secs(300))
            .send()
            .await
            .map_err(|_| anyhow!("wbx upload request failed"))?;
        if matches!(response.status().as_u16(), 401 | 403) {
            bail!("wbx authentication expired during upload; the request outcome may be uncertain")
        }
        let status = response.status();
        let headers = response.headers().clone();
        let body = response
            .bytes()
            .await
            .context("Could not read wsbx upload response")?;
        if body.len() as u64 > MAX_API_RESPONSE {
            bail!("wbx upload response exceeds the safety limit")
        }
        if !status.is_success() {
            let detail = decrypt_response(&body, &aes_key)
                .ok()
                .and_then(|value| {
                    let code = envelope_code(&value)?;
                    let message = value
                        .get("msg")
                        .and_then(Value::as_str)
                        .unwrap_or("request rejected");
                    Some(format!("{code}: {message}"))
                })
                .or_else(|| {
                    std::str::from_utf8(&body)
                        .ok()
                        .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
                        .filter(|text| !text.is_empty())
                        .map(|text| text.chars().take(800).collect())
                });
            match detail {
                Some(detail) => {
                    bail!("wbx upload returned HTTP {} ({detail})", status.as_u16())
                }
                None => bail!(
                    "wbx upload returned HTTP {} (content-type {}, body {} bytes)",
                    status.as_u16(),
                    headers
                        .get("content-type")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or("unknown"),
                    body.len()
                ),
            }
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
    let route = ReimbursementRoute::from_ywlx(ywlx)?;
    let (bmbh, xmbh) = project_parts(project)?;
    match route {
        ReimbursementRoute::Daily => {
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
        ReimbursementRoute::Smart | ReimbursementRoute::Conference => {
            client
                .post_read(
                    "/common/templateInfo/get",
                    json!({
                        "bmbh": bmbh,
                        "xmbh": xmbh,
                        "ywlx": ywlx
                    }),
                    None,
                )
                .await
        }
        ReimbursementRoute::DomesticTravel => {
            client
                .post_read(
                    "/common/tempInfo",
                    json!({
                        "bmbh": bmbh,
                        "xmbh": xmbh,
                        "ywbh": ywbh,
                        "ywlx": ywlx
                    }),
                    None,
                )
                .await
        }
        ReimbursementRoute::InternationalTravel => {
            client
                .get("/intl/travel/init", json!({"ywbh": ywbh}), None)
                .await
        }
    }
}

async fn bind_shared_invoices(
    client: &mut WsbxClient<'_>,
    ywbh: &str,
    invoice_numbers: &[String],
    contract: &str,
) -> Result<Value> {
    let mut pending = invoice_numbers.to_vec();
    for _ in 0..4 {
        let result = client
            .post(
                "/invoice/binding",
                json!({
                    "dzfphs": pending,
                    "ywbh": ywbh,
                    "czr": user_id(client)?,
                    "czrmc": client.auth.as_ref().map(|auth| auth.user_name.as_str()).unwrap_or(""),
                    "htbh": contract
                }),
                Some("invoice"),
            )
            .await?;
        let rejected = result["data"]["delDzfphs"].as_array().map_or(0, Vec::len);
        if rejected > 0 {
            bail!(
                "WSBX rejected {rejected} invoice(s) while binding; inspect the draft before retrying"
            )
        }
        let next = result["data"]["dzfphs"].as_array();
        let Some(next) = next else {
            return Ok(result);
        };
        pending =
            next.iter()
                .map(|value| {
                    value.as_str().map(str::to_owned).ok_or_else(|| {
                        anyhow!("invoice binding returned a non-string invoice number")
                    })
                })
                .collect::<Result<Vec<_>>>()?;
        if pending.is_empty() {
            return Ok(result);
        }
    }
    bail!("invoice binding did not converge after four explicit server-directed passes; inspect the draft before retrying")
}

async fn identify_invoice_documents(
    client: &mut WsbxClient<'_>,
    file: &Path,
) -> Result<Vec<Value>> {
    let user_id = user_id(client)?;
    let user_name = client
        .auth
        .as_ref()
        .map(|auth| auth.user_name.clone())
        .ok_or_else(auth_error)?;
    let result = client
        .upload(
            "/invoice/identify",
            Some("invoice"),
            file,
            json!({
                "userId": user_id,
                "userName": user_name
            }),
        )
        .await?;
    result["data"]
        .as_array()
        .cloned()
        .ok_or_else(|| anyhow!("Invoice OCR returned no document array"))
}

fn invoice_wallet_matches(invoice: &Value, invoice_number: &str) -> bool {
    ["dzfph", "fphm"]
        .iter()
        .any(|key| invoice.get(*key).and_then(Value::as_str) == Some(invoice_number))
}

async fn invoice_wallet_entry(
    client: &mut WsbxClient<'_>,
    invoice_number: &str,
) -> Result<Option<Value>> {
    let user_id = user_id(client)?;
    for status in ["1", "0"] {
        let result = client
            .post_read(
                "/quick/invoice/getQuickList",
                invoice_wallet_search(&user_id, status, invoice_number, 20, 1),
                Some("invoice"),
            )
            .await?;
        if let Some(invoice) = result["data"]["list"].as_array().and_then(|list| {
            list.iter()
                .find(|invoice| invoice_wallet_matches(invoice, invoice_number))
        }) {
            return Ok(Some(invoice.clone()));
        }
    }
    Ok(None)
}

fn user_id(client: &WsbxClient<'_>) -> Result<String> {
    client
        .auth
        .as_ref()
        .map(|auth| auth.user_id.clone())
        .ok_or_else(auth_error)
}

fn route_catalog() -> Value {
    Value::Array(
        [
            ReimbursementRoute::Daily,
            ReimbursementRoute::Smart,
            ReimbursementRoute::DomesticTravel,
            ReimbursementRoute::Conference,
            ReimbursementRoute::InternationalTravel,
        ]
        .into_iter()
        .map(|route| {
            json!({
                "ywlx": route.ywlx(),
                "name": route.name(),
                "list_route": route.list_route(),
                "save_path": route.save_path(),
                "template_path": route.template_path(),
                "payload_mode": if route == ReimbursementRoute::InternationalTravel {
                    "raw_or_intl_budget"
                } else {
                    "structured_or_raw"
                }
            })
        })
        .collect(),
    )
}

pub fn operations() -> Vec<Value> {
    vec![
        json!({"name":"wsbx.routes","mutating":false,"description":"List supported reimbursement routes and their WSBX endpoints without network access","required":[],"optional":[],"upstream":"local route map derived from the current WSBX frontend"}),
        json!({"name":"wsbx.user","mutating":false,"description":"Verify reimbursement SSO and return the current user profile","required":[],"optional":[],"upstream":"wsbx login/sso + userInfo"}),
        json!({"name":"wsbx.business-types","mutating":false,"description":"List enabled reimbursement business types","required":[],"optional":[],"upstream":"myReceipts/getYwlxs/get"}),
        json!({"name":"wsbx.project.list","mutating":false,"description":"Search projects available to the current user","required":[],"optional":["ywlx","query","department","principal","page_size","include_zero_balance"],"upstream":"common/funds/list"}),
        json!({"name":"wsbx.employee.search","mutating":false,"description":"Search campus employees by employee number or name","required":["query"],"optional":["page_size"],"upstream":"common/employee/list"}),
        json!({"name":"wsbx.draft.list","mutating":false,"description":"List reimbursement drafts or submitted receipts","required":[],"optional":["ywlx","status","page_size","route","delegated","filter_delegation"],"upstream":"todoEs/getTodoLi"}),
        json!({"name":"wsbx.draft.get","mutating":false,"description":"Read a reimbursement draft using the route-specific detail endpoint","required":["ywbh"],"optional":["ywlx"],"upstream":"rmb/info/get or intl/travel/init"}),
        json!({"name":"wsbx.draft.preview","mutating":false,"description":"Build and validate a route-specific save/submit payload locally; daily 052 defaults to paperless online push","required":["ywbh"],"optional":["ywlx","project","phone","purpose","description","items","payments","no_paper","is_online","base_info","invoice_list","paper_items","travel","conference","conference_base","intl","intl_budget","travelers","expense_rows","actual_line","actual_start_date","actual_end_date","payload","operator","operator_name","submit"],"upstream":"local payload builder"}),
        json!({"name":"wsbx.draft.status","mutating":false,"description":"Read the workflow state of a receipt","required":["ywbh"],"optional":[],"upstream":"common/get/ywbxr"}),
        json!({"name":"wsbx.payment.init","mutating":false,"description":"Read server-initialized repayment, card, private-payee, public-transfer, and project-transfer rows","required":["ywbh"],"optional":[],"upstream":"pay/init"}),
        json!({"name":"wsbx.payment.methods","mutating":false,"description":"List payment methods enabled for a reimbursement route","required":[],"optional":["ywlx"],"upstream":"pay/method/get"}),
        json!({"name":"wsbx.approval.workflow","mutating":false,"description":"Read an existing approval workflow or generate WSBX's proposed workflow for a pending receipt","required":["ywbh"],"optional":["ywlx","is_cwzg"],"upstream":"workflow/record/get + approve/workflow/get"}),
        json!({"name":"wsbx.approval.check","mutating":false,"description":"Check approver assignments and workflow rules before final approval submission","required":["ywbh"],"optional":["ywlx","workflow","is_cwzg"],"upstream":"approve/spxxdl/check"}),
        json!({"name":"wsbx.approval.submit","mutating":true,"description":"Finalize receipt state 8 by submitting its checked approval workflow","required":["ywbh"],"optional":["ywlx","workflow","is_cwzg","wzh_flag","note","lcid","lcmc","ywbh2"],"upstream":"approve/spxxdl/submit"}),
        json!({"name":"wsbx.draft.create","mutating":true,"description":"Allocate a new reimbursement business number without submitting a receipt","required":[],"optional":["ywlx"],"upstream":"common/ywbh/get"}),
        json!({"name":"wsbx.draft.template","mutating":false,"description":"Read the route-specific expense template or international-travel initialization data","required":["ywbh","project"],"optional":["ywlx"],"upstream":"zdhb/template/get, common/templateInfo/get, common/tempInfo, or intl/travel/init"}),
        json!({"name":"wsbx.draft.bind-project","mutating":true,"description":"Bind a project to a draft and return its reimbursement-item template","required":["ywbh","project"],"optional":["ywlx","old_project","operator"],"upstream":"common/updateXmInfo"}),
        json!({"name":"wsbx.invoice.identify","mutating":false,"description":"OCR and classify a PDF, OFD, image, or ZIP invoice source without adding it to the receipt wallet","required":["file"],"optional":[],"upstream":"invoice/identify"}),
        json!({"name":"wsbx.invoice.add","mutating":true,"description":"OCR, verify, and add invoice source files to the receipt wallet without creating a reimbursement draft","required":["file"],"optional":["folder_id","sfhx","sfzn","ywbh","ocr_data"],"upstream":"invoice/identify + invoice/verify/save"}),
        json!({"name":"wsbx.invoice.add-witness","mutating":true,"description":"Add a witness/proof person to one or more receipt-wallet invoices","required":["invoice_numbers","witness","witness_name"],"optional":["note"],"upstream":"invoice/addZmr"}),
        json!({"name":"wsbx.invoice.wallet.list","mutating":false,"description":"List invoices in the receipt wallet by pending or used state","required":[],"optional":["query","page_size","page_number","status"],"upstream":"quick/invoice/getQuickList"}),
        json!({"name":"wsbx.invoice.list","mutating":false,"description":"List electronic invoices by pending or used reimbursement state","required":[],"optional":["ywbh","query","page_size","status"],"upstream":"invoice/list"}),
        json!({"name":"wsbx.invoice.bind","mutating":true,"description":"Bind selected electronic invoices using the selected reimbursement route","required":["ywbh","item","invoice_numbers"],"optional":["ywlx","fpid","contract"],"upstream":"zdhb/dzfp/bind or invoice/binding"}),
        json!({"name":"wsbx.invoice.unbind","mutating":true,"description":"Unbind one invoice and its linked route data using the selected reimbursement route","required":["ywbh","invoice_number"],"optional":["ywlx","employee","ticket_state","travel_type"],"upstream":"zdhb/dzfp/mx/delete, clf/gn/dzfpHk/delete, invoice/release/use, or conference cleanup endpoints"}),
        json!({"name":"wsbx.invoice.set-amount","mutating":true,"description":"Set the reimbursed amount of an electronic invoice using the selected reimbursement route","required":["ywbh","invoice_number","amount"],"optional":["ywlx","item"],"upstream":"zdhb/dzfp/sbje/update or intl/travel/invoice/update"}),
        json!({"name":"wsbx.invoice.detail","mutating":false,"description":"Read electronic invoices attached to a reimbursement item","required":["ywbh","item"],"optional":[],"upstream":"zdhb/dzfp/mx/get"}),
        json!({"name":"wsbx.attachment.types","mutating":false,"description":"List required and optional attachment categories for a draft","required":["ywbh"],"optional":[],"upstream":"jh/fj/get"}),
        json!({"name":"wsbx.attachment.list","mutating":false,"description":"List attachments on a draft","required":["ywbh"],"optional":["ywlx"],"upstream":"file/get/list"}),
        json!({"name":"wsbx.attachment.upload","mutating":true,"description":"Upload one attachment to a draft","required":["ywbh","file","attachment_type"],"optional":["ywlx"],"upstream":"file/common/upload/file"}),
        json!({"name":"wsbx.attachment.delete","mutating":true,"description":"Delete one attachment from a draft","required":["attachment"],"optional":["fjly"],"upstream":"file/uploadfile/delete"}),
        json!({"name":"wsbx.draft.save","mutating":true,"description":"Validate and save a route-specific draft; daily 052 defaults to paperless online push","required":["ywbh"],"optional":["ywlx","project","phone","purpose","description","items","payments","self_payment","no_paper","is_online","base_info","invoice_list","paper_items","travel","conference","conference_base","intl","intl_budget","travelers","expense_rows","actual_line","actual_start_date","actual_end_date","payload","operator","operator_name"],"upstream":"pay/whole/submit, clf/gn/tmp/save, or intl/travel/save"}),
        json!({"name":"wsbx.draft.submit","mutating":true,"description":"Submit a fully prepared route-specific reimbursement; daily 052 defaults to online push and requires payment allocation","required":["ywbh"],"optional":["ywlx","project","phone","purpose","description","items","payments","self_payment","no_paper","is_online","base_info","invoice_list","paper_items","travel","conference","conference_base","intl","intl_budget","travelers","expense_rows","actual_line","actual_start_date","actual_end_date","payload","operator","operator_name"],"upstream":"pay/whole/submit, clf/gn/tmp/save, or intl/travel/save"}),
        json!({"name":"wsbx.draft.delete","mutating":true,"description":"Permanently delete a draft by business number","required":["ywbh"],"optional":[],"upstream":"todoEs/receipt/delete"}),
        json!({"name":"wsbx.draft.revoke","mutating":true,"description":"Request withdrawal of a submitted receipt by business number","required":["ywbh"],"optional":[],"upstream":"todoEs/receipt/revoke"}),
        json!({"name":"wsbx.conference.budgets","mutating":false,"description":"Search approved conference budgets","required":[],"optional":["page_size","page_number","is_myself","budget_number","query","br"],"upstream":"hyf/ys/page"}),
        json!({"name":"wsbx.conference.expenses","mutating":false,"description":"Read conference expense rows and associated traveler/travel data","required":["ywbh","project"],"optional":["budget_number"],"upstream":"hyf/gnzcmx/list"}),
        json!({"name":"wsbx.currency.list","mutating":false,"description":"List enabled currencies used by international reimbursement","required":[],"optional":[],"upstream":"currency/list"}),
        json!({"name":"wsbx.currency.rate","mutating":false,"description":"Read the current exchange rate for one currency","required":["currency"],"optional":[],"upstream":"currency/rate/get"}),
        json!({"name":"wsbx.intl.budget","mutating":false,"description":"Query an approved international-travel budget by authorisation code or approval number and traveler","required":[],"optional":["author_code","approval_number","employee","name"],"upstream":"intl/travel/budget/list"}),
        json!({"name":"wsbx.intl.get","mutating":false,"description":"Read an international-travel reimbursement draft including RMB/foreign-currency rows","required":["ywbh"],"optional":[],"upstream":"intl/travel/init"}),
        json!({"name":"wsbx.intl.currency.split","mutating":false,"description":"Ask WSBX to split foreign-currency expenses across travelers","required":["travelers","expense_rows"],"optional":[],"upstream":"intl/travel/wb/split"}),
        json!({"name":"wsbx.intl.invoices","mutating":false,"description":"Read invoices attached to an international-travel reimbursement","required":["ywbh"],"optional":[],"upstream":"intl/travel/invoice/list"}),
        json!({"name":"wsbx.intl.invoice.update","mutating":true,"description":"Update reimbursement amount or expense category for an international invoice","required":["ywbh","invoice_number","amount"],"optional":["item"],"upstream":"intl/travel/invoice/update"}),
        json!({"name":"wsbx.intl.files","mutating":false,"description":"List international-travel supporting files by their serial number","required":["serial_number"],"optional":[],"upstream":"intl/file/list"}),
    ]
}

pub async fn execute(session: &Session, operation: &str, a: &Value) -> Result<Value> {
    if operation == "wsbx.routes" {
        return Ok(json!({
            "routes": route_catalog(),
            "note": "Foreign receipts already paid personally normally use 051; InternationalTravel 1206 starts from an approved international-travel budget and carries separate RMB/foreign-currency rows."
        }));
    }
    if operation == "wsbx.draft.preview" {
        let route = route_arg(a)?;
        let submit = bool_arg(a, "submit", false);
        let operator = a.get("operator").and_then(Value::as_str).unwrap_or("");
        let payload = save_payload(a, route, submit, operator)?;
        return Ok(json!({
            "network_request_sent": false,
            "ywlx": route.ywlx(),
            "route": route.name(),
            "save_path": route.save_path(),
            "submit": submit,
            "payload": payload
        }));
    }
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
            let ywlx = route_arg(a)?.ywlx();
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
        "wsbx.employee.search" => {
            client
                .post_read(
                    "/common/employee/list",
                    json!({
                        "condition": required(a, "query")?,
                        "pageSize": u64_arg(a, "page_size", 20)?,
                        "pageNumber": 1
                    }),
                    None,
                )
                .await
        }
        "wsbx.draft.list" => {
            let route = route_arg(a)?;
            let ywlx = route.ywlx();
            client
                .post_read(
                    "/todoEs/getTodoLi",
                    json!({
                        "pageSize": u64_arg(a, "page_size", 20)?,
                        "pageNumber": 0,
                        "current": 1,
                        "ywlx": ywlx,
                        "zt": a.get("status").and_then(Value::as_str).unwrap_or("ALL-2"),
                        "route": a
                            .get("route")
                            .and_then(Value::as_str)
                            .unwrap_or_else(|| route.list_route()),
                        "dcl": "1",
                        "sflz": a.get("filter_delegation").and_then(Value::as_str).unwrap_or("0"),
                        "delegated": a.get("delegated").and_then(Value::as_str).unwrap_or("")
                    }),
                    None,
                )
                .await
        }
        "wsbx.draft.get" => {
            let ywbh = required(a, "ywbh")?;
            if route_arg(a)? == ReimbursementRoute::InternationalTravel {
                client
                    .get("/intl/travel/init", json!({"ywbh": ywbh}), None)
                    .await
            } else {
                client
                    .get("/rmb/info/get", json!({"ywbh": ywbh}), None)
                    .await
            }
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
        "wsbx.payment.init" => {
            client
                .post_read(
                    "/pay/init",
                    json!({
                        "ywbh": required(a, "ywbh")?,
                        "userName": client.auth.as_ref().map(|auth| auth.user_name.as_str()).unwrap_or(""),
                        "userId": user_id(&client)?
                    }),
                    None,
                )
                .await
        }
        "wsbx.payment.methods" => {
            client
                .get(
                    "/pay/method/get",
                    json!({"ywlx": route_arg(a)?.ywlx()}),
                    None,
                )
                .await
        }
        "wsbx.approval.workflow" => {
            let route = route_arg(a)?;
            approval_workflow(
                &mut client,
                &required(a, "ywbh")?,
                route.ywlx(),
                bool_arg(a, "is_cwzg", true),
            )
            .await
        }
        "wsbx.approval.check" => {
            let ywbh = required(a, "ywbh")?;
            let route = route_arg(a)?;
            let is_cwzg = bool_arg(a, "is_cwzg", true);
            let workflow = if let Some(workflow) = a.get("workflow") {
                normalized_workflow(workflow, false)?
            } else {
                approval_workflow(&mut client, &ywbh, route.ywlx(), is_cwzg).await?["workflow"]
                    .as_array()
                    .cloned()
                    .ok_or_else(|| anyhow!("approval workflow is invalid"))?
            };
            check_approval_workflow(&mut client, &ywbh, &workflow, is_cwzg).await
        }
        "wsbx.approval.submit" => {
            let route = route_arg(a)?;
            submit_approval(&mut client, &required(a, "ywbh")?, route.ywlx(), a).await
        }
        "wsbx.draft.create" => {
            let route = route_arg(a)?;
            let ywlx = route.ywlx();
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
                "route": route.name(),
                "saved": false,
                "next": [
                    "wsbx.draft.bind-project",
                    if route == ReimbursementRoute::InternationalTravel {
                        "wsbx.intl.get"
                    } else {
                        "wsbx.draft.template"
                    },
                    "wsbx.attachment.upload",
                    "wsbx.draft.save"
                ]
            }))
        }
        "wsbx.draft.template" => {
            let ywbh = required(a, "ywbh")?;
            let ywlx = route_arg(a)?.ywlx();
            let project = a
                .get("project")
                .ok_or_else(|| anyhow!("project must be a JSON object"))?;
            project_template(&mut client, &ywbh, ywlx, project).await
        }
        "wsbx.draft.bind-project" => {
            let ywbh = required(a, "ywbh")?;
            let route = route_arg(a)?;
            let ywlx = route.ywlx();
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
                "ywlx": ywlx,
                "route": route.name(),
                "project_bound": true,
                "binding": binding,
                "template": template,
                "template_error": template_error
            }))
        }
        "wsbx.invoice.identify" => {
            let file = invoice_file_path(a)?;
            let documents = identify_invoice_documents(&mut client, &file).await?;
            let summaries = documents
                .iter()
                .map(|document| {
                    Ok(json!({
                        "ocr": sanitized_ocr_documents(&json!({"data":[document]}))?
                            .into_iter()
                            .next()
                            .unwrap_or_else(|| json!({})),
                        "verify_data": invoice_ocr_verify_data(document, &file)?
                    }))
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(json!({
                "file": file.file_name().and_then(|value| value.to_str()),
                "documents": summaries
            }))
        }
        "wsbx.invoice.add" => {
            let file = invoice_file_path(a)?;
            let documents = match a.get("ocr_data") {
                Some(Value::Array(documents)) if !documents.is_empty() => documents.clone(),
                Some(Value::Object(_)) => vec![a["ocr_data"].clone()],
                Some(_) => bail!("ocr_data must be a JSON object or nonempty JSON array"),
                None => identify_invoice_documents(&mut client, &file).await?,
            };
            let user_id = user_id(&client)?;
            let ywbh = a.get("ywbh").and_then(Value::as_str).unwrap_or("");
            let sfhx = a.get("sfhx").and_then(Value::as_str).unwrap_or("");
            let sfzn = a.get("sfzn").and_then(Value::as_str).unwrap_or("1");
            let folder_id = a.get("folder_id").and_then(Value::as_str).unwrap_or("");
            let mut added = Vec::new();
            let mut existing = Vec::new();
            let mut failed = Vec::new();

            for (index, document) in documents.iter().enumerate() {
                let verify_data = invoice_ocr_verify_data(document, &file)?;
                let invoice_number = verify_data["fphm"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        anyhow!("Invoice OCR document {} has no invoice number", index + 1)
                    })?;
                if let Some(invoice) = invoice_wallet_entry(&mut client, invoice_number).await? {
                    existing.push(json!({
                        "invoice_number": invoice_number,
                        "status": invoice.get("zt").cloned().unwrap_or(Value::Null),
                        "business_number": invoice.get("ywbh").cloned().unwrap_or(Value::Null)
                    }));
                    continue;
                }

                let result = client
                    .post(
                        "/invoice/verify/save",
                        json!({
                            "ocrData": serde_json::to_string(&verify_data)?,
                            "ygbh": user_id,
                            "ywbh": ywbh,
                            "sfhx": sfhx,
                            "sfzn": sfzn,
                            "folderId": folder_id
                        }),
                        Some("invoice"),
                    )
                    .await?;
                let status = &result["data"]["status"];
                let succeeded = status.as_i64() == Some(0)
                    || status.as_u64() == Some(0)
                    || status.as_str() == Some("0");
                if succeeded {
                    let invoice = &result["data"]["fpInfo"];
                    added.push(json!({
                        "invoice_number": invoice_number,
                        "amount": invoice.get("zje").cloned().unwrap_or(Value::Null),
                        "invoice_type": invoice.get("type").cloned().unwrap_or(Value::Null),
                        "verified": invoice.get("isyz").cloned().unwrap_or(Value::Null),
                        "source_file_present": (["mongoFjName", "mongoFj"]
                            .iter()
                            .any(|key| invoice
                                .get(*key)
                                .and_then(Value::as_str)
                                .is_some_and(|value| !value.is_empty())))
                    }));
                } else {
                    failed.push(json!({
                        "invoice_number": invoice_number,
                        "message": result["data"]["msg"]
                            .as_str()
                            .or_else(|| result["msg"].as_str())
                            .unwrap_or("WSBX rejected invoice verification")
                    }));
                }
            }

            Ok(json!({
                "file": file.file_name().and_then(|value| value.to_str()),
                "added": added,
                "existing": existing,
                "failed": failed,
                "complete": failed.is_empty()
            }))
        }
        "wsbx.invoice.add-witness" => {
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
            client
                .post(
                    "/invoice/addZmr",
                    json!({
                        "dzfphs": invoice_numbers,
                        "zmr": required(a, "witness")?,
                        "zmrmc": required(a, "witness_name")?,
                        "bz": a.get("note").and_then(Value::as_str).unwrap_or(""),
                        "czr": user_id(&client)?,
                        "czrmc": client.auth.as_ref().map(|auth| auth.user_name.as_str()).unwrap_or("")
                    }),
                    Some("invoice"),
                )
                .await
        }
        "wsbx.invoice.wallet.list" => {
            let user_id = user_id(&client)?;
            client
                .post_read(
                    "/quick/invoice/getQuickList",
                    invoice_wallet_search(
                        &user_id,
                        invoice_wallet_status(a)?,
                        a.get("query").and_then(Value::as_str).unwrap_or(""),
                        u64_arg(a, "page_size", 20)?,
                        u64_arg(a, "page_number", 1)?,
                    ),
                    Some("invoice"),
                )
                .await
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
            let route = route_arg(a)?;
            let ywlx = route.ywlx();
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
            if route == ReimbursementRoute::Daily {
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
                return Ok(json!({
                    "business_number": ywbh,
                    "ywlx": ywlx,
                    "bound": true,
                    "item_assignment": "completed",
                    "result": result["data"]
                }));
            }

            let result = bind_shared_invoices(
                &mut client,
                &ywbh,
                &invoice_numbers,
                a.get("contract").and_then(Value::as_str).unwrap_or(""),
            )
            .await?;
            if route == ReimbursementRoute::Conference {
                for invoice_number in &invoice_numbers {
                    client
                        .post(
                            "/hyf/jjflkmbh/update",
                            json!({
                                "ywbh": ywbh,
                                "fpid": a.get("fpid").and_then(Value::as_str).unwrap_or(""),
                                "dzfph": invoice_number,
                                "jjflkmbh": class_code,
                                "jjflkmmc": class_name,
                                "checked": "1"
                            }),
                            None,
                        )
                        .await
                        .with_context(|| {
                            format!(
                                "invoice {invoice_number} was bound, but conference item assignment failed; inspect the draft before retrying"
                            )
                        })?;
                }
            }
            Ok(json!({
                "business_number": ywbh,
                "ywlx": ywlx,
                "bound": true,
                "item_assignment": if route == ReimbursementRoute::Conference {
                    "completed"
                } else {
                    "use invoice.set-amount or route-specific item fields after binding"
                },
                "result": result["data"]
            }))
        }
        "wsbx.invoice.unbind" => {
            let ywbh = required(a, "ywbh")?;
            let invoice_number = required(a, "invoice_number")?;
            let route = route_arg(a)?;
            let result = match route {
                ReimbursementRoute::Daily => {
                    client
                        .get_mutation(
                            "/zdhb/dzfp/mx/delete",
                            json!({"ywbh": ywbh, "dzfph": invoice_number}),
                            None,
                        )
                        .await?
                }
                ReimbursementRoute::DomesticTravel => {
                    client
                        .get_mutation(
                            "/clf/gn/dzfpHk/delete",
                            json!({
                                "ywbh": ywbh,
                                "dzfph": invoice_number,
                                "ygbh": a.get("employee").and_then(Value::as_str).unwrap_or(""),
                                "pjzt": a.get("ticket_state").and_then(Value::as_str).unwrap_or(""),
                                "ccxz": a
                                    .get("travel_type")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                            }),
                            None,
                        )
                        .await?
                }
                ReimbursementRoute::Smart
                | ReimbursementRoute::Conference
                | ReimbursementRoute::InternationalTravel => {
                    let released = client
                        .post(
                            "/invoice/release/use",
                            json!({
                                "dzfph": invoice_number,
                                "ywbh": ywbh,
                                "czr": user_id(&client)?,
                                "czrmc": client.auth.as_ref().map(|auth| auth.user_name.as_str()).unwrap_or("")
                            }),
                            Some("invoice"),
                        )
                        .await?;
                    if route == ReimbursementRoute::Smart {
                        client
                            .get_mutation(
                                "/rmb/ywbh/dzfph/delete",
                                json!({"ywbh": ywbh, "dzfph": invoice_number}),
                                None,
                            )
                            .await?;
                    } else if route == ReimbursementRoute::Conference {
                        client
                            .get_mutation(
                                "/hyf/delete/yw",
                                json!({"ywbh": ywbh, "dzfph": invoice_number}),
                                None,
                            )
                            .await
                            .with_context(|| {
                                format!(
                                    "invoice {invoice_number} was released, but conference expense cleanup failed; inspect the draft before retrying"
                                )
                            })?;
                        client
                            .get_mutation(
                                "/hyf/clxcry/delete",
                                json!({"ywbh": ywbh, "dzfph": invoice_number}),
                                None,
                            )
                            .await
                            .with_context(|| {
                                format!(
                                    "invoice {invoice_number} was released, but conference traveler cleanup failed; inspect the draft before retrying"
                                )
                            })?;
                    }
                    released
                }
            };
            Ok(json!({"business_number": ywbh, "unbound": true, "result": result["data"]}))
        }
        "wsbx.invoice.set-amount" => {
            let ywbh = required(a, "ywbh")?;
            let invoice_number = required(a, "invoice_number")?;
            let amount = required(a, "amount")?;
            let cents = amount_cents(&json!(amount))?;
            let formatted = format_amount(cents);
            let result = if route_arg(a)? == ReimbursementRoute::InternationalTravel {
                client
                    .post(
                        "/intl/travel/invoice/update",
                        json!({
                            "ywbh": ywbh,
                            "dzfph": invoice_number,
                            "je": formatted,
                            "jjflkmmc": a
                                .get("item")
                                .and_then(|item| item.get("jjflkmmc"))
                                .and_then(Value::as_str)
                                .unwrap_or("")
                        }),
                        None,
                    )
                    .await?
            } else {
                client
                    .get_mutation(
                        "/zdhb/dzfp/sbje/update",
                        json!({
                            "ywbh": ywbh,
                            "dzfph": invoice_number,
                            "dzpjSbje": formatted
                        }),
                        None,
                    )
                    .await?
            };
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
            let ywbh = required(a, "ywbh")?;
            let route = route_arg(a)?;
            let ywlx = route.ywlx();
            if submit && route == ReimbursementRoute::Daily {
                let status = client
                    .get("/common/get/ywbxr", json!({"ywbh": ywbh}), None)
                    .await?;
                if status["data"]["zt"].as_str() == Some("8") {
                    let approval = submit_approval(&mut client, &ywbh, ywlx, a).await?;
                    return Ok(json!({
                        "business_number": ywbh,
                        "ywlx": ywlx,
                        "route": route.name(),
                        "staged": false,
                        "submitted": true,
                        "approval": approval
                    }));
                }
            }
            let operator = a
                .get("operator")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or(user_id(&client)?);
            let mut effective_args = a.clone();
            if effective_args
                .get("operator_name")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                effective_args["operator_name"] = json!(client
                    .auth
                    .as_ref()
                    .map(|auth| auth.user_name.as_str())
                    .unwrap_or(""));
            }
            let mut payload = save_payload(&effective_args, route, submit, &operator)?;
            if let Some(value) = effective_args.get("self_payment") {
                if let Some((account, note)) = self_payment_config(value)? {
                    if route != ReimbursementRoute::Daily {
                        bail!("self_payment is currently supported only for daily route 052")
                    }
                    let (reimbursement, existing_payment) = daily_amount_totals(&payload)?;
                    if existing_payment != 0 {
                        bail!("self_payment cannot be combined with nonzero payment rows")
                    }
                    let employee = client
                        .get("/common/employee", json!({"ygbh": user_id(&client)?}), None)
                        .await?;
                    let row = employee_self_payment_row(
                        &employee["data"],
                        account.as_deref(),
                        &format_amount(reimbursement),
                        &note,
                    )?;
                    payload["dszzList"] = json!([row]);
                }
            }
            if submit && route == ReimbursementRoute::Daily {
                validate_daily_payment_allocation(&payload)?;
            }
            if submit {
                let existing = if route == ReimbursementRoute::InternationalTravel {
                    client
                        .get("/intl/travel/init", json!({"ywbh": ywbh}), None)
                        .await?
                } else {
                    client
                        .get("/common/get/ywbxr", json!({"ywbh": ywbh}), None)
                        .await?
                };
                let state = if route == ReimbursementRoute::InternationalTravel {
                    existing["data"]["base"]["state"].as_str()
                } else {
                    existing["data"]["zt"].as_str()
                }
                .ok_or_else(|| anyhow!("receipt status is unavailable; refusing to submit"))?;
                let ready = if route == ReimbursementRoute::InternationalTravel {
                    matches!(state, "1")
                } else {
                    matches!(state, "0" | "Y1" | "Y4")
                };
                if !ready {
                    bail!("receipt state {state} is not submit-ready; inspect it before retrying")
                }
            }
            if route == ReimbursementRoute::Daily {
                let project = effective_args
                    .get("project")
                    .ok_or_else(|| anyhow!("project must be a JSON object"))?;
                let template = project_template(&mut client, &ywbh, ywlx, project).await?;
                let selected = payload["bxsxList"]
                    .as_array()
                    .ok_or_else(|| anyhow!("Saved reimbursement items are invalid"))?;
                validate_template_items(&template, selected)?;
            }
            payload
                .as_object_mut()
                .ok_or_else(|| anyhow!("Saved reimbursement payload is invalid"))?
                .remove("project");
            let result = client
                .post(route.save_path(), payload, None)
                .await
                .with_context(|| {
                    if submit {
                        "WSBX rejected the submission; inspect the draft status and project data permission before retrying"
                    } else {
                        "WSBX rejected the draft save; inspect the draft before retrying"
                    }
                })?;
            let warnings = result["data"]["warnMessage"].clone();
            let mut response = json!({
                "business_number": ywbh,
                "ywlx": ywlx,
                "route": route.name(),
                "endpoint": route.save_path(),
                "saved": !submit,
                "submitted": submit,
                "warnings": warnings,
                "result": result["data"]
            });
            if submit && route == ReimbursementRoute::Daily {
                response["approval"] = submit_approval(&mut client, &ywbh, ywlx, a)
                    .await
                    .with_context(|| {
                        format!(
                            "receipt {ywbh} reached pending-approval state 8, but final approval submission failed; rerun draft.submit after inspecting the workflow"
                        )
                    })?;
            }
            Ok(response)
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
        "wsbx.conference.budgets" => {
            let page_number = u64_arg(a, "page_number", 0)?;
            let mut data = json!({
                "pageSize": u64_arg(a, "page_size", 20)?,
                "pageNumber": page_number,
                "current": page_number.saturating_add(1),
                "isMyself": a.get("is_myself").and_then(Value::as_str).unwrap_or("1"),
                "xm": client.auth.as_ref().map(|auth| auth.user_name.as_str()).unwrap_or("")
            });
            for (argument, field) in [
                ("budget_number", "ysbhCondition"),
                ("query", "searchCondition"),
                ("br", "br"),
            ] {
                if let Some(value) = a.get(argument) {
                    data[field] = value.clone();
                }
            }
            client.post_read("/hyf/ys/page", data, None).await
        }
        "wsbx.conference.expenses" => {
            let project = a
                .get("project")
                .ok_or_else(|| anyhow!("project must be a JSON object"))?;
            let (bmbh, xmbh) = project_parts(project)?;
            client
                .post_read(
                    "/hyf/gnzcmx/list",
                    json!({
                        "ywlx": "19",
                        "bmbh": bmbh,
                        "xmbh": xmbh,
                        "ywbh": required(a, "ywbh")?,
                        "ysbh": a.get("budget_number").and_then(Value::as_str).unwrap_or("")
                    }),
                    None,
                )
                .await
        }
        "wsbx.currency.list" => client.get("/currency/list", json!({}), None).await,
        "wsbx.currency.rate" => {
            client
                .get(
                    "/currency/rate/get",
                    json!({"hbid": required(a, "currency")?}),
                    None,
                )
                .await
        }
        "wsbx.intl.budget" => {
            let author_code = a.get("author_code").and_then(Value::as_str).unwrap_or("");
            let approval_number = a
                .get("approval_number")
                .and_then(Value::as_str)
                .unwrap_or("");
            let employee = a.get("employee").and_then(Value::as_str).unwrap_or("");
            if author_code.is_empty() && (approval_number.is_empty() || employee.is_empty()) {
                bail!("provide author_code, or both approval_number and employee")
            }
            let mut data = json!({
                "authorCode": author_code,
                "rwpjh": approval_number,
                "ygbh": employee
            });
            if let Some(name) = a.get("name") {
                data["ygxm"] = name.clone();
            }
            client
                .post_read("/intl/travel/budget/list", data, None)
                .await
        }
        "wsbx.intl.get" => {
            client
                .get(
                    "/intl/travel/init",
                    json!({"ywbh": required(a, "ywbh")?}),
                    None,
                )
                .await
        }
        "wsbx.intl.currency.split" => {
            let travelers = a
                .get("travelers")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow!("travelers must be a JSON array"))?;
            let expenses = a
                .get("expense_rows")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow!("expense_rows must be a JSON array"))?;
            client
                .post_read(
                    "/intl/travel/wb/split",
                    json!({"ry": travelers, "wb": expenses}),
                    None,
                )
                .await
        }
        "wsbx.intl.invoices" => {
            client
                .post_read(
                    "/intl/travel/invoice/list",
                    json!({"ywbh": required(a, "ywbh")?}),
                    None,
                )
                .await
        }
        "wsbx.intl.invoice.update" => {
            let ywbh = required(a, "ywbh")?;
            let invoice_number = required(a, "invoice_number")?;
            let cents = amount_cents(&json!(required(a, "amount")?))?;
            let result = client
                .post(
                    "/intl/travel/invoice/update",
                    json!({
                        "ywbh": ywbh,
                        "dzfph": invoice_number,
                        "je": format_amount(cents),
                        "jjflkmmc": a
                            .get("item")
                            .and_then(|item| item.get("jjflkmmc"))
                            .and_then(Value::as_str)
                            .unwrap_or("")
                    }),
                    None,
                )
                .await?;
            Ok(json!({"business_number": ywbh, "updated": true, "result": result["data"]}))
        }
        "wsbx.intl.files" => {
            client
                .get(
                    "/intl/file/list",
                    json!({"serialNumber": required(a, "serial_number")?}),
                    None,
                )
                .await
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
    fn multipart_encryption_wraps_form_fields_in_data() {
        let (encrypted, key) =
            encrypted_upload_fields(&json!({"userId": "fixture", "ywbh": ""})).unwrap();
        let plaintext = aes_decrypt(encrypted["data"].as_str().unwrap(), &key).unwrap();
        let decoded: Value = serde_json::from_str(&plaintext).unwrap();
        assert_eq!(decoded, json!({"data": {"userId": "fixture", "ywbh": ""}}));
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
    fn invoice_wallet_queries_match_the_wallet_page_contract() {
        assert_eq!(
            invoice_wallet_status(&json!({"status":"pending"})).unwrap(),
            "1"
        );
        assert_eq!(
            invoice_wallet_status(&json!({"status":"completed"})).unwrap(),
            "0"
        );
        assert!(invoice_wallet_status(&json!({"status":"invalid"})).is_err());
        let query = invoice_wallet_search("fixture", "1", "INV-1", 20, 2);
        assert_eq!(query["fpStatus"], "1");
        assert_eq!(query["fpxx"], "INV-1");
        assert_eq!(query["pageNumber"], 2);
        assert_eq!(query["folderId"], Value::Null);
    }

    #[test]
    fn invoice_wallet_deduplication_requires_the_exact_invoice_number() {
        assert!(!invoice_wallet_matches(
            &json!({"dzfph": "99990000000000000001-OTHER"}),
            "99990000000000000001"
        ));
        assert!(invoice_wallet_matches(
            &json!({"fphm": "99990000000000000001"}),
            "99990000000000000001"
        ));
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
            ReimbursementRoute::Daily,
            false,
            "fixture-user",
        )
        .unwrap();
        assert_eq!(payload["saveType"], "0");
        assert_eq!(payload["zt"], "0");
        assert_eq!(payload["noPaper"], "1");
        assert_eq!(payload["isOnline"], "1");
        assert_eq!(payload["bxsxType"], "zdhb");
        assert_eq!(payload["dszzList"], json!([]));
    }

    #[test]
    fn daily_invoice_defaults_to_paperless_online_and_allows_explicit_override() {
        let base = json!({
            "ywbh": "YB0000000000001",
            "project": {"bmbh": "999", "xmbh": "FIXTURE"},
            "phone": "10000000000",
            "items": [{
                "jjflkmbh": "302",
                "jjflkmmc": "差旅费",
                "dzpjSbje": "10.00",
                "dzpjFjzs": "1"
            }]
        });
        let default = save_payload(&base, ReimbursementRoute::Daily, true, "fixture-user").unwrap();
        assert_eq!(default["noPaper"], "1");
        assert_eq!(default["isOnline"], "1");
        assert_eq!(default["zt"], "8");

        let mut overridden = base;
        overridden["no_paper"] = json!(false);
        overridden["is_online"] = json!(false);
        let overridden =
            save_payload(&overridden, ReimbursementRoute::Daily, true, "fixture-user").unwrap();
        assert_eq!(overridden["noPaper"], "0");
        assert_eq!(overridden["isOnline"], "0");
        assert_eq!(overridden["zt"], "5");

        let external = save_payload(
            &json!({
                "ywbh": "YB0000000000002",
                "ywlx": "052",
                "payload": {
                    "ywbh": "YB0000000000002",
                    "ybbxBaseInfo": {},
                    "bxsxList": []
                }
            }),
            ReimbursementRoute::Daily,
            true,
            "fixture-user",
        )
        .unwrap();
        assert_eq!(external["noPaper"], "1");
        assert_eq!(external["isOnline"], "1");
        assert_eq!(external["zt"], "8");
    }

    #[test]
    fn daily_submission_requires_matching_payment_allocation() {
        let mut payload = json!({
            "bxsxList": [{"sbje": "21.71"}],
            "cjkList": [],
            "gwkList": [],
            "xmzzList": [],
            "dszzList": [],
            "dgzzList": []
        });
        assert!(validate_daily_payment_allocation(&payload).is_err());
        payload["dszzList"] = json!([{"dje": "21.70"}]);
        assert!(validate_daily_payment_allocation(&payload).is_err());
        payload["dszzList"] = json!([{"dje": "21.71"}]);
        assert!(validate_daily_payment_allocation(&payload).is_ok());
    }

    #[test]
    fn self_payment_uses_an_explicit_or_unique_bank_account() {
        let employee = json!({
            "ygbh": "9999000001",
            "ygmc": "Fixture User",
            "gzkh4": "9999000000000001",
            "yh4": "BOC"
        });
        let row = employee_self_payment_row(&employee, None, "21.71", "报销款").unwrap();
        assert_eq!(row["zffsbh"], "08");
        assert_eq!(row["dfdw"], "9999000001");
        assert_eq!(row["dje"], "21.71");
        assert_eq!(row["dfyh"], "BOC");

        let mut multiple = employee;
        multiple["gzkh1"] = json!("9999000000000002");
        multiple["yh1"] = json!("ICBC");
        assert!(employee_self_payment_row(&multiple, None, "21.71", "报销款").is_err());
        let selected =
            employee_self_payment_row(&multiple, Some("gzkh1"), "21.71", "报销款").unwrap();
        assert_eq!(selected["yhzh"], "9999000000000002");
    }

    #[test]
    fn approval_workflow_records_are_normalized_and_checked() {
        let workflow = normalized_workflow(
            &json!([
                {
                    "rank": "2",
                    "roleId": "role-2",
                    "roleNm": "Project approver",
                    "ygbh": "9999000002",
                    "ygmc": "Approver Two",
                    "bjlc": "1",
                    "tsdx": true,
                    "sfqz": "2",
                    "sfqm": "0",
                    "spfs": "2"
                },
                {
                    "rank": "1",
                    "roleId": "role-1",
                    "roleNm": "Department approver",
                    "ygbh": "9999000001",
                    "ygmc": "Approver One",
                    "bjlc": "0",
                    "tsdx": "1",
                    "sfqz": "2",
                    "sfqm": "0",
                    "spfs": "2"
                }
            ]),
            true,
        )
        .unwrap();
        assert_eq!(workflow[0]["rank"], "1");
        assert_eq!(workflow[0]["TSDX"], "1");
        validate_workflow(&workflow).unwrap();

        let mut missing_notification = workflow;
        missing_notification[0]["TSDX"] = json!("0");
        assert!(validate_workflow(&missing_notification).is_err());
    }

    #[test]
    fn reimbursement_routes_match_the_frontend_contract() {
        assert_eq!(
            ReimbursementRoute::from_ywlx("052").unwrap(),
            ReimbursementRoute::Daily
        );
        assert_eq!(
            ReimbursementRoute::from_ywlx("051").unwrap(),
            ReimbursementRoute::Smart
        );
        assert_eq!(
            ReimbursementRoute::from_ywlx("06").unwrap(),
            ReimbursementRoute::DomesticTravel
        );
        assert_eq!(
            ReimbursementRoute::from_ywlx("19").unwrap(),
            ReimbursementRoute::Conference
        );
        assert_eq!(
            ReimbursementRoute::from_ywlx("1206").unwrap(),
            ReimbursementRoute::InternationalTravel
        );
        assert_eq!(
            ReimbursementRoute::DomesticTravel.save_path(),
            "/clf/gn/tmp/save"
        );
        assert_eq!(
            ReimbursementRoute::InternationalTravel.save_path(),
            "/intl/travel/save"
        );
        assert!(ReimbursementRoute::from_ywlx("999").is_err());
    }

    #[test]
    fn smart_payload_matches_the_general_invoice_route() {
        let payload = save_payload(
            &json!({
                "ywbh": "YB0000000000051",
                "ywlx": "051",
                "project": {"bmbh": "999", "xmbh": "FIXTURE"},
                "phone": "10000000000",
                "purpose": "境外票据",
                "description": "个人卡支付",
                "is_online": true,
                "items": [{"jjflkmbh": "302", "jjflkmmc": "测试费", "jje": "10.00"}],
                "invoice_list": [{"dzfph": "INV-1", "bxje": "10.00"}]
            }),
            ReimbursementRoute::Smart,
            true,
            "fixture-user",
        )
        .unwrap();
        assert_eq!(payload["saveType"], "1");
        assert_eq!(payload["isOnline"], "1");
        assert_eq!(payload["czr"], "fixture-user");
        assert_eq!(
            payload["invoiceList"].as_array().unwrap().len(),
            1,
            "foreign invoices already paid personally stay in the 051 route"
        );
        assert!(payload.get("bxsxType").is_none());
    }

    #[test]
    fn domestic_travel_payload_uses_its_own_save_contract() {
        let payload = save_payload(
            &json!({
                "ywbh": "YB0000000000006",
                "ywlx": "06",
                "project": {"bmbh": "999", "xmbh": "FIXTURE"},
                "phone": "10000000000",
                "travel": {
                    "cllx": "1",
                    "ccsy": "学术交流",
                    "ccrq": ["2026-10-01", "2026-10-03"],
                    "ts": "3",
                    "ccdd": "上海",
                    "cczmrbh": "T0001",
                    "cczmrmc": "测试用户",
                    "invoiceList": [{"dzfph": "TRAIN-1", "bxje": "100.00"}],
                    "fpjInfoList": []
                },
                "base_info": {"bmbh": "999", "xmbh": "FIXTURE"}
            }),
            ReimbursementRoute::DomesticTravel,
            false,
            "fixture-user",
        )
        .unwrap();
        assert_eq!(payload["ywlx"], "06");
        assert_eq!(payload["cllx"], "1");
        assert_eq!(payload["invoiceList"].as_array().unwrap().len(), 1);
        assert_eq!(payload["loginUserId"], "fixture-user");
    }

    #[test]
    fn conference_payload_preserves_expense_and_base_blocks() {
        let payload = save_payload(
            &json!({
                "ywbh": "YB0000000000019",
                "ywlx": "19",
                "project": {"bmbh": "999", "xmbh": "FIXTURE"},
                "phone": "10000000000",
                "conference": {
                    "hyfBaseInfo": {
                        "hymc": "测试会议",
                        "hydd": "校内",
                        "hylb": "1",
                        "hykssj": "2026-10-01",
                        "hyjssj": "2026-10-02"
                    },
                    "hyfZcmxList": [{"jjflkmbh": "302", "jjflkmmc": "会议费"}]
                }
            }),
            ReimbursementRoute::Conference,
            false,
            "fixture-user",
        )
        .unwrap();
        assert_eq!(payload["hyfBaseInfo"]["ywbh"], "YB0000000000019");
        assert_eq!(payload["hyfZcmxList"].as_array().unwrap().len(), 1);
        assert_eq!(payload["zt"], "0");
    }

    #[test]
    fn international_payload_accepts_budget_result_and_currency_rows() {
        let payload = save_payload(
            &json!({
                "ywbh": "YB000000001206",
                "ywlx": "1206",
                "project": {"bmbh": "999", "xmbh": "FIXTURE"},
                "intl_budget": {
                    "data": {
                        "base": {"actualLine": "北京-东京", "actualStartDate": "2026-09-01", "actualEndDate": "2026-09-05"},
                        "ry": [{"ygbh": "T0001", "xm": "测试用户"}],
                        "wb": [{"item": "注册费", "detail": [{"actualCurrency": "USD", "actualMoney": "100"}]}],
                        "currencyRate": {"USD": {"rate": "7.1"}}
                    }
                }
            }),
            ReimbursementRoute::InternationalTravel,
            true,
            "fixture-user",
        )
        .unwrap();
        assert_eq!(payload["base"]["ywbh"], "YB000000001206");
        assert_eq!(payload["base"]["ywlx"], "1206");
        assert_eq!(payload["ry"].as_array().unwrap().len(), 1);
        assert_eq!(payload["wb"].as_array().unwrap().len(), 1);
        assert_eq!(payload["currencyRate"]["USD"]["rate"], "7.1");
        assert_eq!(payload["pay"]["ywlx"], "1206");
        assert_eq!(payload["pay"]["saveType"], "1");
        assert_eq!(payload["pay"]["loginUserId"], "fixture-user");
    }

    #[test]
    fn external_payload_cannot_change_the_business_number_or_save_type() {
        let payload = save_payload(
            &json!({
                "ywbh": "YB-CORRECT",
                "ywlx": "19",
                "payload": {
                    "ywbh": "YB-CORRECT",
                    "saveType": "0",
                    "hyfBaseInfo": {"ywbh": "YB-CORRECT"},
                    "hyfZcmxList": [{"jjflkmbh": "302"}]
                }
            }),
            ReimbursementRoute::Conference,
            true,
            "fixture-user",
        )
        .unwrap();
        assert_eq!(payload["saveType"], "1");
        assert!(save_payload(
            &json!({
                "ywbh": "YB-CORRECT",
                "ywlx": "19",
                "payload": {"ywbh": "YB-OTHER"}
            }),
            ReimbursementRoute::Conference,
            false,
            "fixture-user",
        )
        .is_err());
    }

    #[test]
    fn mutation_catalogue_guards_receipt_changes() {
        let operations = operations();
        for name in [
            "wsbx.draft.create",
            "wsbx.draft.bind-project",
            "wsbx.invoice.add",
            "wsbx.invoice.add-witness",
            "wsbx.invoice.bind",
            "wsbx.attachment.upload",
            "wsbx.draft.save",
            "wsbx.draft.submit",
            "wsbx.draft.delete",
            "wsbx.draft.revoke",
            "wsbx.intl.invoice.update",
        ] {
            assert!(
                operations
                    .iter()
                    .any(|operation| operation["name"] == name && operation["mutating"] == true),
                "{name}"
            );
        }
    }

    #[test]
    fn invoice_wallet_ingestion_is_part_of_the_cli_contract() {
        let operations = operations();
        for (name, required) in [
            ("wsbx.employee.search", &["query"][..]),
            ("wsbx.invoice.identify", &["file"][..]),
            ("wsbx.invoice.add", &["file"][..]),
            (
                "wsbx.invoice.add-witness",
                &["invoice_numbers", "witness", "witness_name"][..],
            ),
            ("wsbx.invoice.wallet.list", &[][..]),
        ] {
            let operation = operations
                .iter()
                .find(|operation| operation["name"] == name)
                .unwrap_or_else(|| panic!("{name} must be discoverable"));
            let declared = operation["required"].as_array().unwrap();
            for argument in required {
                assert!(
                    declared.iter().any(|value| value == argument),
                    "{name} must require {argument}"
                );
            }
        }
        assert!(operations
            .iter()
            .any(|operation| operation["name"] == "wsbx.invoice.add"
                && operation["mutating"] == true));
    }

    #[test]
    fn payment_initialization_is_discoverable_and_read_only() {
        let operations = operations();
        for name in ["wsbx.payment.init", "wsbx.payment.methods"] {
            let operation = operations
                .iter()
                .find(|operation| operation["name"] == name)
                .unwrap_or_else(|| panic!("{name} must be discoverable"));
            assert_eq!(operation["mutating"], false);
        }
        assert_eq!(
            operations
                .iter()
                .find(|operation| operation["name"] == "wsbx.payment.init")
                .unwrap()["required"],
            json!(["ywbh"])
        );
    }

    #[test]
    fn final_approval_operations_are_in_the_guarded_catalogue() {
        let operations = operations();
        for name in ["wsbx.approval.workflow", "wsbx.approval.check"] {
            let operation = operations
                .iter()
                .find(|operation| operation["name"] == name)
                .unwrap_or_else(|| panic!("{name} must be discoverable"));
            assert_eq!(operation["mutating"], false);
        }
        let submit = operations
            .iter()
            .find(|operation| operation["name"] == "wsbx.approval.submit")
            .unwrap();
        assert_eq!(submit["mutating"], true);
        assert_eq!(submit["required"], json!(["ywbh"]));
    }

    #[test]
    fn digital_invoice_ocr_is_normalized_for_verification() {
        let data = invoice_ocr_verify_data(
            &json!({
                "code": "normal",
                "fphm": "99990000000000000001",
                "fpje": "21.71",
                "sqje": "19.21",
                "fpjym": "ABC1234567",
                "fplx": "50",
                "kprq": "2026-10-19",
                "gmfmc": "清华大学",
                "xsfmc": "Fixture Trading Co., Ltd."
            }),
            Path::new("digital_99990000000000000001.pdf"),
        )
        .unwrap();
        assert_eq!(data["fphm"], "99990000000000000001");
        assert_eq!(data["kprq"], "20261019");
        assert_eq!(data["jym"], "234567");
        assert_eq!(data["invType"], "50");
        assert_eq!(data["zje"], "21.71");
        assert_eq!(data["fileName"], "digital_99990000000000000001.pdf");
        assert_eq!(data["fileType"], "pdf");
        assert!(data.get("code").is_none());
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
