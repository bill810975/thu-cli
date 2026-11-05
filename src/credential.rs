use anyhow::{bail, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};
use zeroize::{Zeroize, Zeroizing};

const VERSION: u32 = 1;
const MAX_CREDENTIAL_BYTES: u64 = 1024 * 1024;

#[derive(Deserialize, Serialize)]
struct CredentialFile {
    version: u32,
    #[serde(default)]
    accounts: Vec<StoredCredential>,
}

#[derive(Deserialize, Serialize)]
struct StoredCredential {
    username: String,
    password: String,
}

impl Drop for StoredCredential {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

impl Default for CredentialFile {
    fn default() -> Self {
        Self {
            version: VERSION,
            accounts: Vec::new(),
        }
    }
}

pub fn path_for_session(session_path: &Path) -> PathBuf {
    session_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("credentials.json")
}

fn validate_username(username: &str) -> Result<()> {
    if username.is_empty() || !username.chars().all(|character| character.is_ascii_digit()) {
        bail!("username must be your numeric student/staff ID")
    }
    Ok(())
}

fn secure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
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

fn load(path: &Path) -> Result<CredentialFile> {
    if !path.exists() {
        return Ok(CredentialFile::default());
    }
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        bail!("Credential file must not be a symlink")
    }
    if fs::metadata(path)?.len() > MAX_CREDENTIAL_BYTES {
        bail!("Credential file exceeds the safety limit")
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
            bail!(
                "Credential file permissions are too broad; run chmod 600 on {}",
                path.display()
            )
        }
    }
    let document: CredentialFile = serde_json::from_reader(BufReader::new(
        File::open(path).context("Cannot open the local credential file")?,
    ))
    .context("Cannot parse the local credential JSON; run `credential set` again")?;
    if document.version != VERSION {
        bail!(
            "Unsupported local credential file version {}; expected {VERSION}",
            document.version
        )
    }
    for (index, account) in document.accounts.iter().enumerate() {
        validate_username(&account.username)?;
        if account.password.is_empty() {
            bail!("Credential entry {index} has an empty password")
        }
        if document.accounts[..index]
            .iter()
            .any(|other| other.username == account.username)
        {
            bail!("Credential file contains a duplicate username")
        }
    }
    Ok(document)
}

fn acquire_lock(path: &Path) -> Result<File> {
    secure_parent(path)?;
    let lock_path = path.with_file_name("credentials.lock");
    if lock_path.exists() && fs::symlink_metadata(&lock_path)?.file_type().is_symlink() {
        bail!("Credential lock must not be a symlink")
    }
    let mut lock_options = OpenOptions::new();
    lock_options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        lock_options.mode(0o600);
    }
    let lock = lock_options.open(lock_path)?;
    lock.try_lock_exclusive()
        .context("Credential file is in use by another process")?;
    Ok(lock)
}

fn write_unlocked(path: &Path, document: &CredentialFile) -> Result<()> {
    secure_parent(path)?;
    if path.exists() && fs::symlink_metadata(path)?.file_type().is_symlink() {
        bail!("Credential file must not be a symlink")
    }
    let temp = path.with_extension(format!("tmp.{}", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(&temp)
        .context("Cannot create the private credential file")?;
    let result = (|| -> Result<()> {
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, document)
            .context("Cannot serialize the local credential file")?;
        writer.flush()?;
        writer.get_ref().sync_all()?;
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub fn set(path: &Path, username: &str, password: &str) -> Result<()> {
    validate_username(username)?;
    if password.is_empty() {
        bail!("password must not be empty")
    }
    let _lock = acquire_lock(path)?;
    let mut document = load(path)?;
    if let Some(account) = document
        .accounts
        .iter_mut()
        .find(|account| account.username == username)
    {
        account.password.zeroize();
        account.password = password.to_owned();
    } else {
        document.accounts.push(StoredCredential {
            username: username.to_owned(),
            password: password.to_owned(),
        });
    }
    document
        .accounts
        .sort_by(|left, right| left.username.cmp(&right.username));
    write_unlocked(path, &document)
}

pub fn set_interactive(path: &Path, username: &str) -> Result<()> {
    validate_username(username)?;
    let password = Zeroizing::new(rpassword::prompt_password("Password: ")?);
    let confirmation = Zeroizing::new(rpassword::prompt_password("Confirm password: ")?);
    if *password != *confirmation {
        bail!("Passwords did not match; credential was not saved")
    }
    set(path, username, &password)
}

pub fn get(path: &Path, username: &str) -> Result<Option<Zeroizing<String>>> {
    validate_username(username)?;
    let document = load(path)?;
    Ok(document
        .accounts
        .iter()
        .find(|account| account.username == username)
        .map(|account| Zeroizing::new(account.password.clone())))
}

pub fn delete(path: &Path, username: &str) -> Result<bool> {
    validate_username(username)?;
    let _lock = acquire_lock(path)?;
    let mut document = load(path)?;
    let original_len = document.accounts.len();
    document
        .accounts
        .retain(|account| account.username != username);
    if document.accounts.len() == original_len {
        return Ok(false);
    }
    if document.accounts.is_empty() {
        if path.exists() {
            fs::remove_file(path).context("Cannot remove the local credential file")?;
        }
        return Ok(true);
    }
    write_unlocked(path, &document)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn plaintext_roundtrip_is_owner_only() {
        let directory = tempfile::tempdir().unwrap();
        let path = path_for_session(&directory.path().join("info.json"));
        set(&path, "2023000000", "fixture-password").unwrap();
        assert_eq!(
            get(&path, "2023000000").unwrap().unwrap().as_str(),
            "fixture-password"
        );
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"username\""));
        assert!(text.contains("\"password\""));
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(delete(&path, "2023000000").unwrap());
        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn broad_permissions_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("credentials.json");
        fs::write(
            &path,
            r#"{"version":1,"accounts":[{"username":"2023000000","password":"fixture"}]}"#,
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(get(&path, "2023000000").is_err());
    }
}
