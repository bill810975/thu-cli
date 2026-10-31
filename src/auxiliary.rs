//! Remaining app-only external services. No live requests are made in tests.
use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde_json::{json, Value};
pub fn operations() -> Value {
    json!([
     {"name":"water-user","mutating":false,"description":"Water account name/address lookup; upstream uses unencrypted HTTP","required":["id","allow_insecure_http"],"optional":[],"upstream":"THU app network/water.ts:getWaterUserInformation"},
     {"name":"water-order","mutating":true,"description":"Submit bottled-water/ticket order over upstream unencrypted HTTP; can create a paid obligation","required":["id","num","num1","lid","address","allow_insecure_http"],"optional":[],"upstream":"THU app network/water.ts:postWaterSubmission"},
     {"name":"water-brands","mutating":false,"description":"Source-defined water brand identifiers","required":[],"optional":[],"upstream":"THU app network/water.ts:waterBrandIdToName"},
     {"name":"learnx-update","mutating":false,"description":"Read learnX release manifest; does not install an application","required":[],"optional":[],"upstream":"learnX src/helpers/update.ts:getLatestRelease"}
    ])
}
fn arg<'a>(a: &'a Value, k: &str) -> Result<&'a str> {
    a[k].as_str()
        .filter(|s| !s.trim().is_empty())
        .with_context(|| format!("Missing nonempty string argument {k}"))
}
fn insecure(a: &Value) -> Result<()> {
    if a["allow_insecure_http"] != true {
        bail!("Upstream water service sends ID/address over unencrypted HTTP. Set allow_insecure_http=true only if you accept this exposure.")
    }
    Ok(())
}
pub async fn execute(c: &Client, op: &str, a: &Value) -> Result<Value> {
    match op {
 "water-brands"=>Ok(json!({"6":"清紫源泉矿泉水（高端）","10":"燕园泉矿泉水（高端）","12":"农夫山泉桶装水（19L）","11":"清紫源泉矿泉水","8":"喜士天然矿泉水（大）","9":"喜士天然矿泉水（小）","1":"娃哈哈矿泉水","7":"娃哈哈纯净水","5":"清紫源泉纯净水"})),
 "water-user"=>{insecure(a)?;let r=c.post("http://dingshui.bjqzhd.com/auser/getuser.html").form(&[("name","pw"),("param",arg(a,"id")?)]).send().await?.error_for_status()?;Ok(r.json().await?)},
 "water-order"=>{insecure(a)?;for k in ["num","num1"]{let n=arg(a,k)?.parse::<u32>().context("Water quantities must be nonnegative integer strings")?;if n>100{bail!("Water quantity exceeds safety limit 100")}}
 let fields=[("pw",arg(a,"id")?),("num",arg(a,"num")?),("num1",arg(a,"num1")?),("lid",arg(a,"lid")?),("address",arg(a,"address")?)];
 let text=c.post("http://dingshui.bjqzhd.com/buy/subs.html").form(&fields).send().await?.error_for_status()?.text().await?;
 if !text.contains("成功"){bail!("Water provider did not confirm order success; check order history before retrying")}
 Ok(json!({"server_reported_success":true}))},
 "learnx-update"=>Ok(c.get("https://mirrors.tuna.tsinghua.edu.cn/github-release/robertying/learnX/LatestRelease/latest.json").send().await?.error_for_status()?.json().await?),
 _=>bail!("Unknown auxiliary operation")}
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plain_http_requires_explicit_acceptance() {
        assert!(insecure(&json!({})).is_err());
        assert!(insecure(&json!({"allow_insecure_http":true})).is_ok());
    }
}
