//! Release archive builder. Uses locked Cargo metadata for dependency notices.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet, env, fs, os::unix::fs::PermissionsExt, path::Path, process::Command,
};

fn run(command: &mut Command) -> Result<()> {
    ensure!(command.status()?.success(), "Packaging command failed");
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<_> = env::args().collect();
    ensure!(args.len() == 3, "Usage: package BINARY OUTPUT_DIRECTORY");
    let binary = Path::new(&args[1]).canonicalize()?;
    let out = Path::new(&args[2]);
    fs::create_dir_all(out)?;
    let name = format!(
        "firefox-codex-mcp-{}-linux-x86_64",
        env!("CARGO_PKG_VERSION")
    );
    let stage = out.join(&name);
    ensure!(!stage.exists(), "Use an empty output directory");
    fs::create_dir(&stage)?;
    fs::copy(&binary, stage.join("firefox-codex-mcp"))?;
    for (source, target) in [
        ("packaging/install.sh", "install.sh"),
        ("packaging/INSTALL.txt", "INSTALL.txt"),
        ("LICENSE", "LICENSE"),
        ("README.md", "README.md"),
        ("packaging/SIGNING.md", "SIGNING.md"),
        ("PRIVACY.md", "PRIVACY.md"),
    ] {
        fs::copy(source, stage.join(target))
            .with_context(|| format!("Required package file: {source}"))?;
    }
    let signed_name = "codex-work-tabs-0.3.0.xpi";
    let signed = Path::new("packaging/signed").join(signed_name);
    let expected_manifest: Value = serde_json::from_slice(&fs::read("extension/manifest.json")?)?;
    for file in [
        "manifest.json",
        "protocol.js",
        "observation.js",
        "page.js",
        "content.js",
        "native/background.js",
        "native/popup.html",
        "native/popup.css",
        "native/popup.js",
    ] {
        let output = Command::new("unzip")
            .args(["-p"])
            .arg(&signed)
            .arg(file)
            .output()?;
        ensure!(output.status.success(), "Cannot inspect signed XPI");
        if file == "manifest.json" {
            ensure!(
                serde_json::from_slice::<Value>(&output.stdout)? == expected_manifest,
                "Signed manifest differs from source"
            );
        } else {
            ensure!(
                output.stdout == fs::read(Path::new("extension").join(file))?,
                "Signed extension source mismatch: {file}"
            );
        }
    }
    fs::copy(&signed, stage.join(signed_name))?;
    fs::copy(&signed, out.join(signed_name))?;
    // Archives retain executable bits even if the source checkout lost them.
    for file in ["firefox-codex-mcp", "install.sh"] {
        fs::set_permissions(stage.join(file), fs::Permissions::from_mode(0o755))?;
    }
    // Only runtime files: never ship old pairing UI, tests or local credentials.
    let xpi_name = format!(
        "firefox-codex-mcp-{}-unsigned.xpi",
        env!("CARGO_PKG_VERSION")
    );
    let xpi = out.canonicalize()?.join(&xpi_name);
    run(Command::new("zip")
        .current_dir("extension")
        .args(["-q", "-X"])
        .arg(&xpi)
        .args([
            "manifest.json",
            "protocol.js",
            "observation.js",
            "page.js",
            "content.js",
            "native/background.js",
            "native/popup.html",
            "native/popup.css",
            "native/popup.js",
        ]))?;
    fs::copy(&xpi, stage.join(&xpi_name))?;
    let metadata = Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args([
            "metadata",
            "--locked",
            "--offline",
            "--format-version",
            "1",
            "--filter-platform",
            "x86_64-unknown-linux-gnu",
        ])
        .output()?;
    ensure!(metadata.status.success(), "Cargo metadata failed");
    let metadata: Value = serde_json::from_slice(&metadata.stdout)?;
    let mut selected = HashSet::new();
    let root = metadata["packages"]
        .as_array()
        .context("Missing packages")?
        .iter()
        .find(|p| p["name"] == "firefox-codex-mcp" && p["source"].is_null())
        .context("Missing root package")?;
    let mut pending = vec![root["id"].as_str().context("Missing root ID")?.to_owned()];
    while let Some(id) = pending.pop() {
        if !selected.insert(id.clone()) {
            continue;
        }
        let node = metadata["resolve"]["nodes"]
            .as_array()
            .context("Missing dependency graph")?
            .iter()
            .find(|n| n["id"] == id)
            .context("Missing dependency node")?;
        for dep in node["deps"]
            .as_array()
            .context("Missing dependency edges")?
        {
            if dep["dep_kinds"]
                .as_array()
                .context("Missing dependency kinds")?
                .iter()
                .any(|k| k["kind"].is_null() || k["kind"] == "build")
            {
                pending.push(
                    dep["pkg"]
                        .as_str()
                        .context("Missing dependency ID")?
                        .to_owned(),
                );
            }
        }
    }
    let mut components = Vec::new();
    let mut notices = String::from("Linux production and build dependency inventory from Cargo.lock.\nLicense expressions are provided by each upstream crate. Copies of shipped upstream notices follow under licenses/.\n\n");
    for package in metadata["packages"]
        .as_array()
        .context("Missing package metadata")?
    {
        if package["source"].is_null()
            || !selected.contains(package["id"].as_str().context("Missing package ID")?)
        {
            continue;
        }
        let name = package["name"].as_str().context("Missing crate name")?;
        let version = package["version"].as_str().context("Missing version")?;
        let license = package["license"]
            .as_str()
            .context("Dependency has no license metadata; review before distributing")?;
        notices.push_str(&format!("{name} {version}: {license}\n"));
        let source = Path::new(
            package["manifest_path"]
                .as_str()
                .context("Missing manifest")?,
        )
        .parent()
        .context("Missing crate directory")?;
        let dest = stage.join("licenses").join(format!("{name}-{version}"));
        fs::create_dir_all(&dest)?;
        let mut count = 0;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let filename = entry.file_name();
            let upper = filename.to_string_lossy().to_uppercase();
            if entry.file_type()?.is_file()
                && ["LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT"]
                    .iter()
                    .any(|prefix| upper.starts_with(prefix))
            {
                fs::copy(entry.path(), dest.join(filename))?;
                count += 1;
            }
        }
        ensure!(
            count > 0,
            "{name} has no top-level license file; review before distributing"
        );
        components.push(json!({"type":"library","name":name,"version":version,"purl":format!("pkg:cargo/{name}@{version}"),"licenses":[{"expression":license}]}));
    }
    fs::write(stage.join("THIRD_PARTY_NOTICES.txt"), notices)?;
    fs::write(
        stage.join("sbom.cdx.json"),
        serde_json::to_vec_pretty(
            &json!({"bomFormat":"CycloneDX","specVersion":"1.6","version":1,"components":components}),
        )?,
    )?;
    let digest = format!("{:x}", Sha256::digest(fs::read(&binary)?));
    fs::write(
        stage.join("BUILD.json"),
        serde_json::to_vec_pretty(
            &json!({"version":env!("CARGO_PKG_VERSION"),"target":"x86_64-unknown-linux-gnu","binarySha256":digest,"commit":env::var("GITHUB_SHA").ok(),"runner":env::var("RUNNER_OS").ok()}),
        )?,
    )?;
    run(Command::new("tar")
        .arg("-czf")
        .arg(out.join(format!("{name}.tar.gz")))
        .arg("-C")
        .arg(out)
        .arg(&name))?;
    let archive = fs::read(out.join(format!("{name}.tar.gz")))?;
    fs::write(
        out.join("SHA256SUMS"),
        format!(
            "{:x}  {name}.tar.gz\n{:x}  {xpi_name}\n{:x}  {signed_name}\n",
            Sha256::digest(&archive),
            Sha256::digest(fs::read(xpi)?),
            Sha256::digest(fs::read(signed)?)
        ),
    )?;
    println!("{}", out.join(format!("{name}.tar.gz")).display());
    Ok(())
}
