//! Native Linux packages use system-owned payloads and no user-home hooks.
use anyhow::{ensure, Context, Result};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

const DIRECTORY: &str = "usr/libexec/firefox-codex-mcp";
const PACKAGE: &str = "firefox-codex-mcp";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const ACTIVATE: &str = include_str!("../../../../packaging/linux/activate.sh");
const SPEC: &str = include_str!("../../../../packaging/linux/package.spec.in");

fn write(path: &Path, content: impl AsRef<[u8]>, mode: u32) -> Result<()> {
    fs::create_dir_all(path.parent().context("Missing parent directory")?)?;
    fs::write(path, content)?;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}

fn run(command: &mut Command) -> Result<()> {
    let result = command
        .output()
        .with_context(|| format!("Run {command:?}"))?;
    ensure!(
        result.status.success(),
        "{command:?} failed:\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}

fn normalize_payload(path: &Path, root: &Path) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            normalize_payload(&entry.path(), root)?;
        } else if kind.is_file() {
            let executable = entry.path().parent() == Some(&root.join(DIRECTORY));
            fs::set_permissions(
                entry.path(),
                fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }),
            )?;
        } else {
            ensure!(
                kind.is_symlink() && entry.path() == root.join("usr/bin").join(PACKAGE),
                "Unexpected native package link"
            );
        }
    }
    Ok(())
}

fn versions(release: &str) -> Result<(String, String, String)> {
    ensure!(
        VERSION
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c)),
        "Unsupported package version"
    );
    let (base, prerelease) = VERSION
        .split_once('-')
        .map_or((VERSION, None), |(base, pre)| (base, Some(pre)));
    let deb = match prerelease {
        Some(pre) => format!("{base}~{pre}-{release}"),
        None => format!("{base}-{release}"),
    };
    let rpm_release = match prerelease {
        Some(pre) => format!("0.{pre}.{release}"),
        None => release.to_owned(),
    };
    Ok((deb, base.to_owned(), rpm_release))
}

pub fn build(stage: &Path, out: &Path, release: &str) -> Result<Vec<PathBuf>> {
    ensure!(
        std::env::consts::ARCH == "x86_64",
        "Only Linux x86_64 packages are supported"
    );
    // Native builders preserve Debian/RPM metadata and dependency semantics.
    for tool in ["dpkg-deb", "rpmbuild"] {
        run(Command::new(tool).arg("--version"))?;
    }
    let (deb_version, rpm_version, rpm_release) = versions(release)?;
    let out = out.canonicalize()?;
    let work = out.join(format!(".linux-packages-{release}"));
    ensure!(
        !work.exists(),
        "Use an empty output directory for native packages"
    );
    let payload = work.join("payload");
    fs::create_dir_all(payload.join(DIRECTORY))?;
    fs::copy(stage.join(PACKAGE), payload.join(DIRECTORY).join(PACKAGE))?;
    fs::set_permissions(
        payload.join(DIRECTORY).join(PACKAGE),
        fs::Permissions::from_mode(0o755),
    )?;
    write(
        &payload.join(DIRECTORY).join("native-host"),
        include_str!("../../../../packaging/linux/native-host"),
        0o755,
    )?;
    fs::create_dir_all(payload.join("usr/bin"))?;
    write(
        &payload.join("usr/lib/mozilla/native-messaging-hosts/firefox_codex_mcp.json"),
        serde_json::to_vec_pretty(
            &json!({"name":"firefox_codex_mcp","description":"Firefox Codex local connection","path":format!("/{DIRECTORY}/native-host"),"type":"stdio","allowed_extensions":["{a171de16-96b6-4e50-94b7-e0b54d94436a}"]}),
        )?,
        0o644,
    )?;
    let docs = payload.join("usr/share/doc").join(PACKAGE);
    super::copy_directory(stage, &docs)?;
    // The native package has one executable, managed by the package manager.
    for file in [PACKAGE, "install.sh"] {
        fs::remove_file(docs.join(file))?;
    }
    let licenses = payload.join("usr/share/licenses").join(PACKAGE);
    fs::create_dir_all(&licenses)?;
    fs::copy(docs.join("LICENSE"), licenses.join("LICENSE"))?;
    fs::copy(docs.join("LICENSE"), docs.join("copyright"))?;
    super::copy_directory(&docs.join("licenses"), &licenses.join("dependencies"))?;
    normalize_payload(&payload, &payload)?;
    let activate = ACTIVATE.replace("@VERSION@", VERSION).replace(
        "@EXECUTABLE_SHA256@",
        &format!("{:x}", Sha256::digest(fs::read(stage.join(PACKAGE))?)),
    );

    // Debian control files live outside the common payload copied into RPM.
    let deb_root = work.join("deb-root");
    super::copy_directory(&payload, &deb_root)?;
    normalize_payload(&deb_root, &deb_root)?;
    // copy_directory intentionally disallows input symlinks. Add only this
    // fixed, package-owned command link after copying the regular payload.
    for root in [&payload, &deb_root] {
        std::os::unix::fs::symlink(
            format!("/{DIRECTORY}/{PACKAGE}"),
            root.join("usr/bin").join(PACKAGE),
        )?;
    }
    let control = deb_root.join("DEBIAN");
    fs::create_dir_all(&control)?;
    fs::set_permissions(&control, fs::Permissions::from_mode(0o755))?;
    write(&control.join("control"), format!("Package: {PACKAGE}\nVersion: {deb_version}\nSection: web\nPriority: optional\nArchitecture: amd64\nMaintainer: firefox-codex-mcp contributors <238377850+nobadwordss@users.noreply.github.com>\nDepends: libc6 (>= 2.35), libgcc-s1, coreutils\nHomepage: https://github.com/nobadwordss/firefox-codex-mcp\nDescription: Codex work tabs in your normal Firefox\n Native Messaging helper for Codex Work Tabs. Manage the per-user Codex\n connection through the extension Settings in your existing Firefox.\n Package installation never changes Firefox profiles or user homes.\n"), 0o644)?;
    for (name, text) in [
        (
            "preinst",
            include_str!("../../../../packaging/linux/preinst"),
        ),
        ("prerm", include_str!("../../../../packaging/linux/prerm")),
        ("postrm", include_str!("../../../../packaging/linux/postrm")),
    ] {
        write(&control.join(name), text, 0o755)?;
    }
    write(
        &control.join("postinst"),
        format!(
            "#!/bin/sh\nset -eu\ncase \"${{1:-}}\" in\nconfigure|abort-upgrade|abort-remove|abort-deconfigure)\n{activate}\n;;\nesac\nexit 0\n"
        ),
        0o755,
    )?;
    let deb = out.join(format!("{PACKAGE}_{deb_version}_amd64.deb"));
    run(Command::new("dpkg-deb")
        .args(["--root-owner-group", "--build"])
        .arg(&deb_root)
        .arg(&deb))?;

    // Fedora's 64-bit Firefox also accepts the architecture-specific directory.
    // Debian keeps its documented /usr/lib registration only.
    let rpm_manifest =
        payload.join("usr/lib64/mozilla/native-messaging-hosts/firefox_codex_mcp.json");
    fs::create_dir_all(rpm_manifest.parent().context("Missing manifest parent")?)?;
    fs::copy(
        payload.join("usr/lib/mozilla/native-messaging-hosts/firefox_codex_mcp.json"),
        rpm_manifest,
    )?;
    normalize_payload(&payload, &payload)?;

    let rpm_root = work.join("rpm");
    for directory in [
        "BUILD",
        "BUILDROOT",
        "RPMS",
        "SOURCES",
        "SPECS",
        "SRPMS",
        "tmp",
    ] {
        fs::create_dir_all(rpm_root.join(directory))?;
    }
    let spec = SPEC
        .replace("@RPM_VERSION@", &rpm_version)
        .replace("@RPM_RELEASE@", &rpm_release)
        .replace(
            "@PAYLOAD@",
            &format!(
                "'{}'/.",
                payload
                    .to_str()
                    .context("Non-UTF-8 build directory")?
                    .replace('%', "%%")
                    .replace('\'', "'\\''")
            ),
        )
        // RPM expands percent escapes even inside shell parameter expansion.
        .replace("@ACTIVATE@", &activate.replace('%', "%%"));
    let spec_path = rpm_root.join("SPECS/package.spec");
    write(&spec_path, spec, 0o644)?;
    run(Command::new("rpmbuild")
        .arg("-bb")
        .arg("--define")
        .arg(format!("_topdir {}", rpm_root.display()))
        .arg("--define")
        .arg(format!("_tmppath {}", rpm_root.join("tmp").display()))
        .args([
            "--define",
            "_build_id_links none",
            "--define",
            "_binary_payload w9.gzdio",
            "--define",
            "_binary_filedigest_algorithm 8",
        ])
        .arg(&spec_path))?;
    let rpm_name = format!("{PACKAGE}-{rpm_version}-{rpm_release}.x86_64.rpm");
    let rpm = out.join(&rpm_name);
    fs::copy(rpm_root.join("RPMS/x86_64").join(&rpm_name), &rpm)?;
    let dump = Command::new("rpm")
        .args(["-qp", "--qf", "[%{FILENAMES} %{FILEDIGESTS}\\n]"])
        .arg(&rpm)
        .output()?;
    ensure!(
        dump.status.success(),
        "Could not inspect RPM payload digests"
    );
    let expected = format!(
        "/{DIRECTORY}/{PACKAGE} {:x}",
        Sha256::digest(fs::read(stage.join(PACKAGE))?)
    );
    ensure!(
        String::from_utf8(dump.stdout)?
            .lines()
            .any(|line| line == expected),
        "RPM builder changed the reviewed executable"
    );
    fs::remove_dir_all(work)?;
    Ok(vec![deb, rpm])
}
