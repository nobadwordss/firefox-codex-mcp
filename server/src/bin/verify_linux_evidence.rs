//! Require completed Linux release scenarios in CI, independently of pipeline logs.
use anyhow::{ensure, Context, Result};
use serde_json::Value;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

const RECORDS: &[(&str, &[&str])] = &[
    (
        "continuity-result.json",
        &[
            "sameTabAcrossProcesses",
            "independentFormStateReadback",
            "overlapFencing",
            "crossChatDenied",
            "revokeIrreversible",
            "signed",
        ],
    ),
    (
        "result.json",
        &[
            "firstInstall",
            "signedInstallUi",
            "extensionSettingsInSameFirefoxProfile",
            "settingsUpdateResponseSurvivesNativeReset",
            "permanentSignedRestartVerified",
        ],
    ),
    (
        "settings-removal-result.json",
        &[
            "responseDeliveredAfterNativeReset",
            "serviceClosed",
            "firefoxFixturePreserved",
            "registrationRemoved",
        ],
    ),
    (
        "codex-app-server-result.json",
        &[
            "actualCodexProcess",
            "actualFirefox",
            "packageManagedBinary",
            "systemNativeManifest",
            "independentDomReadback",
            "chatIsolation",
            "revoke",
        ],
    ),
    (
        "cold-extension-first/result.json",
        &[
            "receiptAbsentBeforeSettings",
            "codexConfigurationAbsentBeforeSettings",
            "toolbarSettingsColdStartedHelper",
            "connectViaSettingsUi",
            "doctorViaSettingsUi",
            "actualCodexAppServerFirstWebTask",
            "disconnectViaSettingsUi",
            "systemPackagePreservedAfterUserDisconnect",
        ],
    ),
    (
        "cold-extension-first/codex-app-server-result.json",
        &[
            "actualCodexProcess",
            "actualFirefox",
            "packageManagedBinary",
            "systemNativeManifest",
            "independentDomReadback",
            "chatIsolation",
            "revoke",
        ],
    ),
];

fn read(root: &Path, record: &str) -> Result<Value> {
    let path = root.join(record);
    let bytes =
        fs::read(&path).with_context(|| format!("Missing release evidence: {}", path.display()))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("Invalid release evidence: {}", path.display()))
}
fn browser(root: &Path) -> Result<()> {
    for (record, fields) in RECORDS {
        let path = root.join(record);
        let value = read(root, record)?;
        for field in *fields {
            ensure!(
                value[*field] == true,
                "Incomplete browser evidence: {}: {field}",
                path.display()
            );
        }
    }
    println!(
        "Verified {} completed browser evidence records in {}",
        RECORDS.len(),
        root.display()
    );
    Ok(())
}
fn lifecycle(root: &Path) -> Result<()> {
    for (record, format) in [
        ("ubuntu/deb-lifecycle.json", "deb"),
        ("debian/deb-lifecycle.json", "deb"),
        ("fedora/rpm-lifecycle.json", "rpm"),
    ] {
        let value = read(root, record)?;
        ensure!(
            value["completed"] == true
                && value["format"] == format
                && value["personalProfilesUsed"] == false,
            "Incomplete package lifecycle evidence: {record}"
        );
        ensure!(
            value["sourceVersion"]
                .as_str()
                .is_some_and(|v| !v.is_empty())
                && value["legacyPortableVersion"] == "0.4.2",
            "Missing package lifecycle version identity: {record}"
        );
    }
    println!(
        "Verified 3 completed package lifecycle evidence records in {}",
        root.display()
    );
    Ok(())
}
fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let mode = args
        .next()
        .context("Usage: verify_linux_evidence browser|lifecycle DIRECTORY")?;
    let root = PathBuf::from(
        args.next()
            .context("Usage: verify_linux_evidence browser|lifecycle DIRECTORY")?,
    );
    ensure!(
        args.next().is_none(),
        "Usage: verify_linux_evidence browser|lifecycle DIRECTORY"
    );
    match mode.to_str() {
        Some("browser") => browser(&root),
        Some("lifecycle") => lifecycle(&root),
        _ => anyhow::bail!("Usage: verify_linux_evidence browser|lifecycle DIRECTORY"),
    }
}
