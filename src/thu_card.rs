//! Native card API adapter derived from thu-info-lib/card.ts (upstream BSL).
use aes::Aes128;
use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyInit};
use reqwest::Client;
use serde_json::{json, Value};
const ROOT: &str = "https://card.tsinghua.edu.cn";
fn required(a: &Value, k: &str) -> Result<String> {
    a.get(k)
        .filter(|v| !v.is_null())
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| v.to_string())
        })
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("missing argument: {k}"))
}
fn parse_response(v: Value) -> Result<Value> {
    if v["success"] == true {
        return Ok(v["resultData"].clone());
    }
    let data = v["data"]
        .as_str()
        .ok_or_else(|| anyhow!("card response rejected or requires authentication"))?;
    if data.len() < 17 || !data.is_char_boundary(16) {
        bail!("invalid encrypted card response");
    }
    let key = &data.as_bytes()[..16];
    let encrypted = STANDARD
        .decode(&data[16..])
        .context("invalid card response base64")?;
    let plaintext = ecb::Decryptor::<Aes128>::new_from_slice(key)
        .map_err(|_| anyhow!("invalid card response key"))?
        .decrypt_padded_vec_mut::<Pkcs7>(&encrypted)
        .map_err(|_| anyhow!("invalid card response padding"))?;
    let inner: Value =
        serde_json::from_slice(&plaintext).context("invalid decrypted card response JSON")?;
    if inner["success"] != true {
        bail!(
            "card API operation rejected: {}",
            inner["message"].as_str().unwrap_or("unknown error")
        );
    }
    Ok(inner["resultData"].clone())
}
async fn fetch(c: &Client, path: &str, p: Value) -> Result<Value> {
    let r = c
        .post(format!("{ROOT}{path}"))
        .json(&p)
        .send()
        .await?
        .error_for_status()?;
    let v: Value = r
        .json()
        .await
        .context("card API returned non-JSON; establish card session")?;
    parse_response(v)
}
async fn user(c: &Client) -> Result<String> {
    let v = fetch(c, "/login/getUserInfoFromToken", json!({})).await?;
    required(&v, "loginuser").context("no active campus card session")
}
async fn info(c: &Client, u: &str) -> Result<Value> {
    fetch(c, "/business/getCardUserinfo", json!({"idserial":u})).await
}
fn cents(a: &Value, k: &str) -> Result<u64> {
    let n = a
        .get(k)
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("{k} must be a non-negative integer in cents"))?;
    Ok(n)
}
fn amount(a: &Value) -> Result<f64> {
    let n = required(a, "amount")?.parse::<f64>()?;
    if !n.is_finite() || n <= 0.0 {
        bail!("amount must be positive yuan");
    }
    if ((n * 100.0).round() - n * 100.0).abs() > 0.00001 {
        bail!("amount cannot have more than two decimal places");
    }
    Ok(n)
}
fn summary(raw: &Value) -> Value {
    let c = &raw["cardInfos"][0];
    let yuan = |v: &Value| v.as_f64().map(|v| v / 100.0);
    json!({"userId":raw["idserial"],"userName":raw["username"],"userNameEn":raw["engname"],"departmentName":raw["departname"],"departmentNameEn":raw["engdepartname"],"departmentId":raw["departid"],"phoneNumber":raw["tel"],"userGender":raw["sex"],"effectiveTimestamp":raw["identifyeffectdate"],"validTimestamp":raw["validatevalue"],"balance":yuan(&raw["baseAccount"]["balance"]),"cardId":c["cardid"],"cardStatus":c["accstatus"],"lastTransactionTimestamp":c["lasttxdate"],"maxDailyTransactionAmount":yuan(&c["maxconstolamt"]),"maxOneTimeTransactionAmount":yuan(&c["maxconsamt"])})
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
        ("card.user", false, "Current card service account", &[], &[]),
        (
            "card.info",
            false,
            "Card identity, balance and transaction limits in yuan",
            &[],
            &["raw"],
        ),
        ("card.photo", false, "Authenticated photo URL", &[], &[]),
        (
            "card.transactions",
            false,
            "Transaction page; amounts normalized to yuan",
            &["start", "end"],
            &["type", "page", "page_size"],
        ),
        (
            "card.password-change",
            true,
            "Change transaction password",
            &["old_password", "new_password"],
            &[],
        ),
        (
            "card.limits",
            true,
            "Set card maxconsamt and maxconstolamt explicitly in cents",
            &[
                "transaction_password",
                "maxconsamt_cents",
                "maxconstolamt_cents",
            ],
            &[],
        ),
        (
            "card.report-loss",
            true,
            "Report card loss",
            &["transaction_password"],
            &[],
        ),
        (
            "card.cancel-loss",
            true,
            "Cancel card loss report",
            &["transaction_password"],
            &[],
        ),
        (
            "card.recharge-supported",
            false,
            "Check upstream card API compatibility",
            &[],
            &[],
        ),
        (
            "card.recharge-bank",
            true,
            "Transfer funds from bound bank to campus card",
            &["amount"],
            &[],
        ),
        (
            "card.recharge-qr",
            true,
            "Create WeChat/Alipay recharge QR handoff; no payment completion",
            &["amount", "provider"],
            &[],
        ),
    ];
    Value::Array(defs.iter().map(|(n,m,d,r,o)|json!({"name":n,"mutating":m,"description":d,"required":r,"optional":o,"upstream":"thu-info-lib/src/lib/card.ts"})).collect())
}
pub async fn execute(c: &Client, operation: &str, a: &Value) -> Result<Value> {
    if operation == "card.recharge-supported" {
        let v: Value = c
            .get("https://app.cs.tsinghua.edu.cn/api/CardIVersion")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let version = v["version"]
            .as_u64()
            .ok_or_else(|| anyhow!("missing card API version"))?;
        return Ok(json!({"server_version":version,"client_version":2,"supported":version<=2}));
    }
    if operation == "card.recharge-bank" || operation == "card.recharge-qr" {
        let version: Value = c
            .get("https://app.cs.tsinghua.edu.cn/api/CardIVersion")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if version["version"].as_u64().is_none_or(|v| v > 2) {
            bail!("campus card recharge API compatibility could not be confirmed; upgrade/check official service");
        }
    }
    let u = user(c).await?;
    match operation{
 "card.user"=>Ok(json!({"loginuser":u})),
 "card.info"=>{let v=info(c,&u).await?;Ok(if a["raw"]==true {v}else{summary(&v)})},
 "card.photo"=>{let v=info(c,&u).await?;let id=required(&v["cardInfos"][0],"cardid")?;let mut url=reqwest::Url::parse(&format!("{ROOT}/myaccount/showDbImage"))?;url.query_pairs_mut().append_pair("idserial",&id);Ok(json!({"url":url.as_str(),"requires_session":true}))},
 "card.transactions"=>{let page=a["page"].as_u64().unwrap_or(0);let size=a["page_size"].as_u64().unwrap_or(100);if size==0||size>10000{bail!("page_size must be 1..10000");}let v=fetch(c,"/business/querySelfTradeList",json!({"idserial":u,"starttime":required(a,"start")?,"endtime":required(a,"end")?,"tradetype":a.get("type").cloned().unwrap_or(json!(-1)),"pageSize":size,"pageNumber":page})).await?;let rows=v["rows"].as_array().ok_or_else(||anyhow!("card response missing rows"))?.iter().map(|r|json!({"id":r["id"],"summary":r["summary"],"timestamp":r["txdate"],"balance":r["balance"].as_f64().map(|n|n/100.0),"amount":r["txamt"].as_f64().map(|n|n/100.0),"address":r["meraddr"],"name":r["mername"],"txName":r["txname"]})).collect::<Vec<_>>();Ok(json!({"page":page,"page_size":size,"total":v.get("total").or_else(||v.get("totalCount")),"rows":rows,"pagination":"Request the next zero-based page until fewer than page_size rows are returned."}))},
 "card.password-change"=>fetch(c,"/business/modifyPwdByPhoneVerify",json!({"idserial":u,"oldpassword":required(a,"old_password")?,"txpassword":required(a,"new_password")?,"authOldPwd":true})).await,
 "card.limits"=>{let v=info(c,&u).await?;fetch(c,"/business/modifyCardMaxConsamt",json!({"cardid":v["cardInfos"][0]["cardid"],"txpassword":required(a,"transaction_password")?,"maxconsamt":cents(a,"maxconsamt_cents")?,"maxconstolamt":cents(a,"maxconstolamt_cents")?})).await},
 "card.report-loss"|"card.cancel-loss"=>fetch(c,if operation=="card.report-loss"{"/business/cardReportLoss"}else{"/business/solutionHang"},json!({"idserial":u,"txpasswd":required(a,"transaction_password")?})).await,
 "card.recharge-bank"=>{let n=amount(a)?;let v=fetch(c,"/business/moblieRecharge",json!({"idserial":u,"txamt":(n*100.0).round() as u64})).await?;if v["returncode"]=="ERROR"{bail!("bank recharge rejected; use official service or its permitted time window");}Ok(v)},
 "card.recharge-qr"=>{let n=amount(a)?;let provider=required(a,"provider")?;let p=match provider.as_str(){"alipay"=>json!({"idserial":u,"transamt":n,"paytype":3,"txcode":"2493","productdesc":"综合服务支付宝扫码充值","method":"trade.pay.qrcode","tradetype":"alipay.qrcode"}),"wechat"=>json!({"idserial":u,"transamt":n,"txamt":n,"openid":"","orgid":2,"paytype":2,"txcode":"1824","productdesc":"综合服务微信扫码充值","method":"trade.pay.qrcode","tradetype":"weixin.qrcode"}),_=>bail!("provider must be alipay or wechat")};let v=fetch(c,"/wx/rechard/qrcode",p).await?;if v["success"]!=true{bail!("payment QR creation rejected");}let response:Value=serde_json::from_str(v["response"].as_str().ok_or_else(||anyhow!("missing payment response"))?)?;let url=response["bizContent"]["webUrl"].as_str().ok_or_else(||anyhow!("missing payment URL"))?;Ok(json!({"provider":provider,"payment_url":url,"payment_completed":false}))},
 _=>bail!("unknown card operation: {operation}"),
 }
}
#[cfg(test)]
mod tests {
    use super::*;
    use cipher::BlockEncryptMut;
    #[test]
    fn plain_response() {
        assert_eq!(
            parse_response(json!({"success":true,"resultData":{"x":1}})).unwrap()["x"],
            1
        );
    }
    #[test]
    fn encrypted_response() {
        let key = b"0123456789abcdef";
        let ct = ecb::Encryptor::<Aes128>::new_from_slice(key)
            .unwrap()
            .encrypt_padded_vec_mut::<Pkcs7>(br#"{"success":true,"resultData":{"balance":500}}"#);
        let data = format!("0123456789abcdef{}", STANDARD.encode(ct));
        assert_eq!(
            parse_response(json!({"data":data})).unwrap()["balance"],
            500
        );
    }
    #[test]
    fn malformed_response() {
        assert!(parse_response(json!({"data":"short"})).is_err());
        assert!(parse_response(json!({"success":false})).is_err());
    }
    #[test]
    fn exact_cents() {
        assert_eq!(amount(&json!({"amount":"10.25"})).unwrap(), 10.25);
        assert!(amount(&json!({"amount":"10.251"})).is_err());
        assert!(amount(&json!({"amount":"-2"})).is_err());
    }
}
