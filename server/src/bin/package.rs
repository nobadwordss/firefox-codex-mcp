//! Release archive builder. Uses locked Cargo metadata for dependency notices.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, env, fs, path::Path, process::Command};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const RUNTIME_FILES: &[&str] = &[
    "manifest.json",
    "protocol.js",
    "observation.js",
    "page.js",
    "content.js",
    "native/background.js",
    "native/popup.html",
    "native/popup.css",
    "native/popup.js",
    "icons/cursor-16.png",
    "icons/cursor-32.png",
    "icons/cursor-48.png",
    "icons/cursor-96.png",
    "icons/cursor-128.png",
];

#[cfg(unix)]
fn run(command: &mut Command) -> Result<()> {
    ensure!(command.status()?.success(), "Packaging command failed");
    Ok(())
}
fn copy_directory(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_directory(&entry.path(), &target.join(entry.file_name()))?;
        } else {
            ensure!(
                kind.is_file(),
                "Only regular documentation files may be packaged"
            );
            fs::copy(entry.path(), target.join(entry.file_name()))?;
        }
    }
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<_> = env::args().collect();
    ensure!(args.len() == 3, "Usage: package BINARY OUTPUT_DIRECTORY");
    let binary = Path::new(&args[1]).canonicalize()?;
    let out = Path::new(&args[2]);
    fs::create_dir_all(out)?;
    let name = format!(
        "firefox-codex-mcp-{}-{}-x86_64",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    );
    let stage = out.join(&name);
    ensure!(!stage.exists(), "Use an empty output directory");
    fs::create_dir(&stage)?;
    fs::copy(
        &binary,
        stage.join(if cfg!(windows) {
            "firefox-codex-mcp.exe"
        } else {
            "firefox-codex-mcp"
        }),
    )?;
    for (source, target) in [
        (
            if cfg!(windows) {
                "packaging/INSTALL.windows.txt"
            } else {
                "packaging/INSTALL.txt"
            },
            "INSTALL.txt",
        ),
        ("LICENSE", "LICENSE"),
        ("README.md", "README.md"),
        ("README.ko.md", "README.ko.md"),
        ("CHANGELOG.md", "CHANGELOG.md"),
        ("packaging/SIGNING.md", "SIGNING.md"),
        ("PRIVACY.md", "PRIVACY.md"),
    ] {
        fs::copy(source, stage.join(target))
            .with_context(|| format!("Required package file: {source}"))?;
    }
    #[cfg(unix)]
    fs::copy("packaging/install.sh", stage.join("install.sh"))?;
    copy_directory(Path::new("docs"), &stage.join("docs"))?;
    copy_directory(Path::new("extension/icons"), &stage.join("extension/icons"))?;
    fs::create_dir_all(stage.join("packaging"))?;
    fs::copy("packaging/SIGNING.md", stage.join("packaging/SIGNING.md"))?;
    let signed_name = "codex-work-tabs-0.4.0.xpi";
    let signed = Path::new("packaging/signed").join(signed_name);
    let expected_manifest: Value = serde_json::from_slice(&fs::read("extension/manifest.json")?)?;
    for &file in RUNTIME_FILES {
        use std::io::Read;
        let mut archive = zip::ZipArchive::new(fs::File::open(&signed)?)?;
        let mut output = Vec::new();
        archive.by_name(file)?.read_to_end(&mut output)?;
        if file == "manifest.json" {
            ensure!(
                serde_json::from_slice::<Value>(&output)? == expected_manifest,
                "Signed manifest differs from source"
            );
        } else {
            ensure!(
                output == fs::read(Path::new("extension").join(file))?,
                "Signed extension source mismatch: {file}"
            );
        }
    }
    fs::copy(&signed, stage.join(signed_name))?;
    fs::copy(&signed, out.join(signed_name))?;
    // Archives retain executable bits even if the source checkout lost them.
    #[cfg(unix)]
    for file in ["firefox-codex-mcp", "install.sh"] {
        fs::set_permissions(stage.join(file), fs::Permissions::from_mode(0o755))?;
    }
    // Only runtime files: never ship old pairing UI, tests or local credentials.
    let xpi_name = format!(
        "firefox-codex-mcp-{}-unsigned.xpi",
        env!("CARGO_PKG_VERSION")
    );
    let xpi = out.canonicalize()?.join(&xpi_name);
    let mut unsigned = zip::ZipWriter::new(fs::File::create(&xpi)?);
    for &file in RUNTIME_FILES {
        unsigned.start_file(
            file,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )?;
        std::io::copy(
            &mut fs::File::open(Path::new("extension").join(file))?,
            &mut unsigned,
        )?;
    }
    unsigned.finish()?;
    fs::copy(&xpi, stage.join(&xpi_name))?;
    let metadata = Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args([
            "metadata",
            "--locked",
            "--offline",
            "--format-version",
            "1",
            "--filter-platform",
            if cfg!(windows) {
                "x86_64-pc-windows-msvc"
            } else {
                "x86_64-unknown-linux-gnu"
            },
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
    let mut notices = String::from("Production and build dependency inventory from Cargo.lock.\nLicense expressions are provided by each upstream crate. Copies of shipped upstream notices follow under licenses/.\n\n");
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
            &json!({"version":env!("CARGO_PKG_VERSION"),"target":if cfg!(windows) {"x86_64-pc-windows-msvc"} else {"x86_64-unknown-linux-gnu"},"binarySha256":digest,"commit":env::var("GITHUB_SHA").ok(),"runner":env::var("RUNNER_OS").ok()}),
        )?,
    )?;
    let suffix = if cfg!(windows) { "zip" } else { "tar.gz" };
    let archive_path = out.join(format!("{name}.{suffix}"));
    #[cfg(unix)]
    run(Command::new("tar")
        .arg("-czf")
        .arg(&archive_path)
        .arg("-C")
        .arg(out)
        .arg(&name))?;
    #[cfg(windows)]
    {
        fn add(writer: &mut zip::ZipWriter<fs::File>, path: &Path, root: &Path) -> Result<()> {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    add(writer, &entry.path(), root)?;
                } else {
                    writer.start_file(
                        entry
                            .path()
                            .strip_prefix(root)?
                            .to_string_lossy()
                            .replace('\\', "/"),
                        zip::write::SimpleFileOptions::default()
                            .compression_method(zip::CompressionMethod::Deflated),
                    )?;
                    std::io::copy(&mut fs::File::open(entry.path())?, writer)?;
                }
            }
            Ok(())
        }
        let mut writer = zip::ZipWriter::new(fs::File::create(&archive_path)?);
        add(&mut writer, &stage, out)?;
        writer.finish()?;
        fs::copy(&binary, out.join(format!("{name}-setup.exe")))?;
    }
    let mut sums = format!(
        "{:x}  {name}.{suffix}\n{:x}  {xpi_name}\n{:x}  {signed_name}\n",
        Sha256::digest(fs::read(&archive_path)?),
        Sha256::digest(fs::read(xpi)?),
        Sha256::digest(fs::read(signed)?)
    );
    if cfg!(windows) {
        sums.push_str(&format!("{digest}  {name}-setup.exe\n"));
    }
    fs::write(out.join("SHA256SUMS"), sums)?;
    println!("{}", archive_path.display());
    Ok(())
}
