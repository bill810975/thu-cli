use anyhow::{bail, Context, Result};
use reqwest::Client;
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

pub struct Session {
    pub client: Client,
    pub auth_client: Client,
    jar: Arc<CookieStoreMutex>,
    path: PathBuf,
    _lock: File,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AuthProfile {
    pub username: String,
    pub fingerprint: String,
    pub service: String,
    #[serde(default)]
    pub use_webvpn: bool,
}

pub fn default_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("edu", "tsinghua", "thu-cli")
        .context("Cannot locate user configuration directory; set --session")?;
    Ok(dirs.config_dir().join("cookies.json"))
}

fn secure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
            }
        }
    }
    Ok(())
}

impl Session {
    pub fn open(path: PathBuf) -> Result<Self> {
        secure_parent(&path)?;
        let lock_path = path.with_extension("lock");
        if lock_path.exists() && fs::symlink_metadata(&lock_path)?.file_type().is_symlink() {
            bail!("Session lock must not be a symlink")
        }
        let mut lock_options = OpenOptions::new();
        lock_options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            lock_options.mode(0o600);
        }
        let lock = lock_options.open(lock_path)?;
        fs2::FileExt::try_lock_exclusive(&lock).context(
            "Session is in use by another process; wait or use a separate --session profile",
        )?;
        let store = if path.exists() {
            if fs::symlink_metadata(&path)?.file_type().is_symlink() {
                bail!("Session file must not be a symlink");
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if fs::metadata(&path)?.permissions().mode() & 0o077 != 0 {
                    bail!(
                        "Session file permissions are too broad; run chmod 600 on the session file"
                    );
                }
            }
            cookie_store::serde::json::load(BufReader::new(File::open(&path)?)).map_err(|_| {
                anyhow::anyhow!(
                    "Cannot parse session cookie store; back up/remove it or import a valid store"
                )
            })?
        } else {
            CookieStore::default()
        };
        let jar = Arc::new(CookieStoreMutex::new(store));
        let client = Client::builder().cookie_provider(jar.clone())
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/79.0.3945.88 Safari/537.36")
            .timeout(Duration::from_secs(60)).connect_timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                let target=attempt.url();
                let source=attempt.previous().last();
                let host=target.host_str().unwrap_or_default();
                let school=|h:&str|h=="tsinghua.edu.cn" || h.ends_with(".tsinghua.edu.cn");
                if attempt.previous().len()>=15 {attempt.error("Too many redirects")}
                else if source.is_some_and(|u|u.scheme()=="https") && target.scheme()!="https" {attempt.error("Refusing HTTPS downgrade")}
                else if source.is_some_and(|u|u.host_str()==target.host_str() || (school(u.host_str().unwrap_or_default()) && school(host)) || ((school(u.host_str().unwrap_or_default()) || u.host_str().is_some_and(|h|h=="alipay.com"||h.ends_with(".alipay.com"))) && (host=="alipay.com"||host.ends_with(".alipay.com")))) {attempt.follow()}
                else {attempt.error("Refusing cross-domain redirect; inspect destination before retrying")}
            })).build()?;
        let auth_client = Client::builder().cookie_provider(jar.clone())
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/79.0.3945.88 Safari/537.36").timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                let u=attempt.url();
                let host=u.host_str().unwrap_or_default();
                if attempt.previous().len()>=15 { attempt.error("Too many authentication redirects") }
                else if u.scheme()=="https" && (host=="tsinghua.edu.cn" || host.ends_with(".tsinghua.edu.cn")) {attempt.follow()}
                else {attempt.error("Authentication redirect outside trusted HTTPS campus domains")}
            })).build()?;
        Ok(Self {
            client,
            auth_client,
            jar,
            path,
            _lock: lock,
        })
    }

    pub fn save(&self) -> Result<()> {
        secure_parent(&self.path)?;
        let temp = self
            .path
            .with_extension(format!("tmp.{}", std::process::id()));
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let file = opts
            .open(&temp)
            .context("Cannot create private session checkpoint")?;
        let result = (|| -> Result<()> {
            let mut writer = BufWriter::new(file);
            cookie_store::serde::json::save_incl_expired_and_nonpersistent(
                &*self
                    .jar
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Cookie store lock poisoned"))?,
                &mut writer,
            )
            .map_err(|_| anyhow::anyhow!("Cannot serialize cookies"))?;
            writer.flush()?;
            writer.get_ref().sync_all()?;
            fs::rename(&temp, &self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }

    pub fn status(&self) -> Result<serde_json::Value> {
        let jar = self
            .jar
            .lock()
            .map_err(|_| anyhow::anyhow!("Cookie store lock poisoned"))?;
        let count = jar.iter_unexpired().count();
        Ok(
            serde_json::json!({"path": self.path,"unexpired_cookies":count,"authenticated":null,"note":"Cookie presence does not verify a live authenticated session"}),
        )
    }

    pub fn load_profile(&self) -> Result<Option<AuthProfile>> {
        let path = self.path.with_extension("auth.json");
        if !path.exists() {
            return Ok(None);
        }
        if fs::symlink_metadata(&path)?.file_type().is_symlink() {
            bail!("Authentication profile must not be a symlink")
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if fs::metadata(&path)?.permissions().mode() & 0o077 != 0 {
                bail!("Authentication profile permissions are too broad; run chmod 600 on it")
            }
        }
        let profile = serde_json::from_reader(BufReader::new(File::open(&path)?))
            .context("Cannot parse authentication profile; run login again")?;
        Ok(Some(profile))
    }

    pub fn save_profile(&self, profile: &AuthProfile) -> Result<()> {
        secure_parent(&self.path)?;
        let path = self.path.with_extension("auth.json");
        let temp = path.with_extension(format!("tmp.{}", std::process::id()));
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let file = opts
            .open(&temp)
            .context("Cannot create private authentication profile")?;
        let result = (|| -> Result<()> {
            let mut writer = BufWriter::new(file);
            serde_json::to_writer(&mut writer, profile)
                .context("Cannot serialize authentication profile")?;
            writer.flush()?;
            writer.get_ref().sync_all()?;
            fs::rename(&temp, &path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }

    pub fn clear_profile(&self) -> Result<()> {
        let path = self.path.with_extension("auth.json");
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context("Cannot clear authentication profile"),
        }
    }

    pub fn cookie_value(&self, domain: &str, name: &str) -> Result<Option<String>> {
        let jar = self
            .jar
            .lock()
            .map_err(|_| anyhow::anyhow!("Cookie store lock poisoned"))?;
        let value = jar
            .iter_unexpired()
            .find(|cookie| {
                cookie.name() == name
                    && cookie
                        .domain
                        .as_cow()
                        .is_some_and(|candidate| candidate.trim_start_matches('.') == domain)
            })
            .map(|cookie| cookie.value().to_owned());
        Ok(value)
    }

    pub fn import_netscape(&self, path: &Path) -> Result<usize> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
                bail!("Cookie import file must have mode 600 or stricter");
            }
        }
        let input = zeroize::Zeroizing::new(fs::read_to_string(path)?);
        let mut staged = CookieStore::default();
        let mut count = 0;
        for (i, line) in input.lines().enumerate() {
            let (line, http_only) = if let Some(l) = line.strip_prefix("#HttpOnly_") {
                (l, true)
            } else {
                (line, false)
            };
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let f: Vec<_> = line.split('\t').collect();
            if f.len() != 7 {
                bail!("Invalid Netscape cookie row {} (contents omitted)", i + 1)
            }
            let host = f[0].trim_start_matches('.');
            if !(host == "tsinghua.edu.cn" || host.ends_with(".tsinghua.edu.cn")) {
                bail!("Cookie import only accepts tsinghua.edu.cn domains");
            }
            if f.iter().any(|x| x.contains(['\r', '\n', ';'])) {
                bail!("Invalid cookie field")
            }
            let expiry: i64 = f[4].parse().context("Invalid cookie expiry")?;
            if expiry != 0 && expiry <= chrono::Utc::now().timestamp() {
                continue;
            }
            let mut raw = format!("{}={}; Path={}", f[5], f[6], f[2]);
            if f[1] == "TRUE" {
                raw.push_str(&format!("; Domain={}", f[0]));
            }
            if f[3] == "TRUE" {
                raw.push_str("; Secure");
            }
            if http_only {
                raw.push_str("; HttpOnly");
            }
            if expiry != 0 {
                raw.push_str(&format!(
                    "; Max-Age={}",
                    expiry - chrono::Utc::now().timestamp()
                ));
            }
            let url = reqwest::Url::parse(&format!("https://{host}/"))?;
            staged
                .parse(&raw, &url)
                .map_err(|_| anyhow::anyhow!("Invalid cookie in row {}", i + 1))?;
            count += 1;
        }
        if count == 0 {
            bail!("No unexpired campus cookies found")
        }
        *self
            .jar
            .lock()
            .map_err(|_| anyhow::anyhow!("Cookie store lock poisoned"))? = staged;
        self.save()?;
        Ok(count)
    }

    pub fn clear(&self) -> Result<()> {
        self.jar
            .lock()
            .map_err(|_| anyhow::anyhow!("Cookie store lock poisoned"))?
            .clear();
        self.save()?;
        self.clear_profile()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_session_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("private/cookies.json");
        let s = Session::open(path.clone()).unwrap();
        s.save().unwrap();
        drop(s);
        assert_eq!(
            Session::open(path.clone()).unwrap().status().unwrap()["unexpired_cookies"],
            0
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn cookie_value_reads_a_host_only_cookie() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cookies.json");
        fs::write(
            &path,
            r#"[
  {
    "raw_cookie": "XSRF-TOKEN=fixture",
    "path": ["/", true],
    "domain": {"HostOnly": "info.tsinghua.edu.cn"},
    "expires": "SessionEnd"
  }
]"#,
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }

        let session = Session::open(path).unwrap();
        assert_eq!(
            session
                .cookie_value("info.tsinghua.edu.cn", "XSRF-TOKEN")
                .unwrap(),
            Some("fixture".to_owned())
        );
    }

    #[test]
    fn authentication_profile_roundtrip_is_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("info.json");
        let session = Session::open(path.clone()).unwrap();
        let profile = AuthProfile {
            username: "fixture-user".to_owned(),
            fingerprint: "fixture-fingerprint".to_owned(),
            service: "info".to_owned(),
            use_webvpn: false,
        };

        session.save_profile(&profile).unwrap();
        assert_eq!(session.load_profile().unwrap(), Some(profile.clone()));
        let profile_path = path.with_extension("auth.json");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&profile_path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }

        session.clear_profile().unwrap();
        assert_eq!(session.load_profile().unwrap(), None);
    }
}
