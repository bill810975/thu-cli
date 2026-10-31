use anyhow::{bail, Context, Result};
use std::process::Stdio;
use zeroize::Zeroizing;

const SECURITY: &str = "/usr/bin/security";
pub const KEYCHAIN_SERVICE: &str = "io.github.bill810975.thu-cli";

#[cfg(target_os = "macos")]
pub fn set_interactive(username: &str) -> Result<()> {
    let status = std::process::Command::new(SECURITY)
        .args([
            "add-generic-password",
            "-U",
            "-a",
            username,
            "-s",
            KEYCHAIN_SERVICE,
            "-w",
        ])
        .status()
        .context("Could not start macOS Keychain helper")?;
    if !status.success() {
        bail!("Could not save the campus credential in macOS Keychain");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn get(username: &str) -> Result<Zeroizing<String>> {
    let output = std::process::Command::new(SECURITY)
        .args([
            "find-generic-password",
            "-a",
            username,
            "-s",
            KEYCHAIN_SERVICE,
            "-w",
        ])
        .output()
        .context("Could not start macOS Keychain helper")?;
    if !output.status.success() {
        bail!("No accessible campus credential in macOS Keychain for this account");
    }
    let mut value = Zeroizing::new(output.stdout);
    while value
        .last()
        .is_some_and(|byte| matches!(byte, b'\r' | b'\n'))
    {
        value.pop();
    }
    let password = Zeroizing::new(
        String::from_utf8(value.to_vec()).context("Stored Keychain credential is not UTF-8")?,
    );
    if password.is_empty() {
        bail!("Stored Keychain credential is empty");
    }
    Ok(password)
}

#[cfg(target_os = "macos")]
pub fn delete(username: &str) -> Result<()> {
    let status = std::process::Command::new(SECURITY)
        .args([
            "delete-generic-password",
            "-a",
            username,
            "-s",
            KEYCHAIN_SERVICE,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("Could not start macOS Keychain helper")?;
    if !status.success() {
        bail!("No accessible campus credential in macOS Keychain for this account");
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn set_interactive(_username: &str) -> Result<()> {
    bail!("macOS Keychain credential storage is only available on macOS")
}

#[cfg(not(target_os = "macos"))]
pub fn get(_username: &str) -> Result<Zeroizing<String>> {
    bail!("macOS Keychain credential storage is only available on macOS")
}

#[cfg(not(target_os = "macos"))]
pub fn delete(_username: &str) -> Result<()> {
    bail!("macOS Keychain credential storage is only available on macOS")
}
