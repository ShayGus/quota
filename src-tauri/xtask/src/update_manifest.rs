//! `update-manifest`: the update list a release publishes, built and verified.
//!
//! The installed application reads `latest.json`, finds its platform, downloads
//! the package named there, and installs it only if the signature in the list
//! verifies against the public key compiled into it. This command makes the
//! list from the packages and signatures the build jobs produced, and refuses a
//! release whose list could not be installed by anyone:
//!
//! - `assemble` writes `latest.json` from the release directory;
//! - `verify` checks it: every required platform is present, each signature is
//!   the one on disk and verifies the package under the repository's public
//!   key, each address names an asset of the same release, and the version is
//!   the one in `tauri.conf.json`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use minisign_verify::{PublicKey, Signature};
use serde_json::{Map, Value, json};

use crate::check_updater::DOWNLOAD_BASE;
use crate::outcome::Outcome;
use crate::scan;

const CONFIG: &str = "src-tauri/tauri.conf.json";
const MANIFEST: &str = "latest.json";

/// One kind of package, and the update-list keys it answers.
///
/// The first key is the generic one for the platform. The second is the exact
/// installer kind the plugin looks up first, so a copy installed from an MSI or
/// a deb is given its own package, not the installer of another kind.
struct Package {
    suffix: &'static str,
    keys: &'static [&'static str],
}

const PACKAGES: [Package; 7] = [
    Package {
        suffix: "-setup.exe",
        keys: &["windows-x86_64", "windows-x86_64-nsis"],
    },
    Package {
        suffix: ".msi",
        keys: &["windows-x86_64-msi"],
    },
    Package {
        suffix: ".AppImage",
        keys: &["linux-x86_64", "linux-x86_64-appimage"],
    },
    Package {
        suffix: ".deb",
        keys: &["linux-x86_64-deb"],
    },
    Package {
        suffix: ".rpm",
        keys: &["linux-x86_64-rpm"],
    },
    Package {
        suffix: "_aarch64.app.tar.gz",
        keys: &["darwin-aarch64", "darwin-aarch64-app"],
    },
    Package {
        suffix: "_x64.app.tar.gz",
        keys: &["darwin-x86_64", "darwin-x86_64-app"],
    },
];

/// The platforms every release must update.
pub(crate) const REQUIRED: [&str; 4] = [
    "windows-x86_64",
    "linux-x86_64",
    "darwin-aarch64",
    "darwin-x86_64",
];

/// Decodes base64 that holds UTF-8 text, as the key and signature files do.
pub(crate) fn decode_base64_text(encoded: &str) -> Option<String> {
    let bytes = STANDARD.decode(encoded.trim()).ok()?;
    String::from_utf8(bytes).ok()
}

/// What the command line asked for.
pub(crate) struct Request<'a> {
    pub(crate) root: &'a Path,
    pub(crate) directory: &'a Path,
    pub(crate) version: Option<&'a str>,
    pub(crate) date: Option<&'a str>,
}

/// The version `tauri.conf.json` declares, checked against any the caller named.
fn configured_version(request: &Request<'_>, outcome: &mut Outcome) -> Option<String> {
    let parsed = scan::read(&request.root.join(CONFIG))
        .and_then(|text| serde_json::from_str::<Value>(&text).map_err(|error| error.to_string()));
    let configured = match parsed {
        Ok(config) => config
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_owned),
        Err(error) => {
            outcome.fail(
                CONFIG.to_string(),
                1,
                format!("cannot read the configuration: {error}"),
            );
            return None;
        }
    };
    let Some(configured) = configured else {
        outcome.fail(
            CONFIG.to_string(),
            1,
            "the configuration declares no version".to_string(),
        );
        return None;
    };
    if let Some(named) = request.version.filter(|named| *named != configured) {
        outcome.fail(
            CONFIG.to_string(),
            1,
            format!("the release is `{named}` but the configuration declares `{configured}`"),
        );
        return None;
    }
    Some(configured)
}

/// The public key the application is built with.
fn configured_key(root: &Path, outcome: &mut Outcome) -> Option<PublicKey> {
    let key = scan::read(&root.join(CONFIG))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|config| {
            config
                .pointer("/plugins/updater/pubkey")?
                .as_str()
                .map(str::to_owned)
        })
        .and_then(|encoded| decode_base64_text(&encoded))
        .and_then(|text| PublicKey::decode(&text).ok());
    if key.is_none() {
        outcome.fail(
            CONFIG.to_string(),
            1,
            "`plugins.updater.pubkey` is not a usable minisign public key".to_string(),
        );
    }
    key
}

/// Whether an asset name is safe to put in an address unescaped.
fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
}

/// The file names in `directory`, sorted.
fn names(directory: &Path) -> Vec<String> {
    let mut found = fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>();
    found.sort();
    found
}

/// Writes `latest.json` into the release directory.
pub(crate) fn assemble(request: &Request<'_>) -> Outcome {
    let mut outcome = Outcome::default();
    let Some(version) = configured_version(request, &mut outcome) else {
        return outcome;
    };
    let present = names(request.directory);
    let mut platforms = Map::new();
    for package in &PACKAGES {
        let assets = present
            .iter()
            .filter(|name| name.ends_with(package.suffix))
            .collect::<Vec<_>>();
        let [asset] = assets.as_slice() else {
            if assets.len() > 1 {
                outcome.fail(
                    request.directory.display().to_string(),
                    1,
                    format!("more than one `*{}` package: {assets:?}", package.suffix),
                );
            }
            continue;
        };
        let signature = fs::read_to_string(request.directory.join(format!("{asset}.sig")));
        let Ok(signature) = signature else {
            outcome.fail(
                format!("{asset}.sig"),
                1,
                format!("`{asset}` has no signature file, so no installed copy could accept it"),
            );
            continue;
        };
        if !plain_name(asset) {
            outcome.fail(
                (*asset).clone(),
                1,
                "the asset name needs escaping in an address".to_string(),
            );
            continue;
        }
        for key in package.keys {
            platforms.insert(
                (*key).to_string(),
                json!({
                    "signature": signature.trim(),
                    "url": format!("{DOWNLOAD_BASE}/v{version}/{asset}"),
                }),
            );
        }
    }
    let mut manifest = Map::new();
    manifest.insert("version".to_string(), json!(version));
    manifest.insert("notes".to_string(), json!(format!("Quota {version}.")));
    if let Some(date) = request.date {
        manifest.insert("pub_date".to_string(), json!(date));
    }
    manifest.insert("platforms".to_string(), Value::Object(platforms));
    let text = serde_json::to_string_pretty(&Value::Object(manifest)).unwrap_or_default();
    match fs::write(request.directory.join(MANIFEST), format!("{text}\n")) {
        Ok(()) => outcome.note(format!("wrote {MANIFEST} for version {version}")),
        Err(error) => outcome.fail(MANIFEST.to_string(), 1, format!("cannot write it: {error}")),
    }
    outcome
}

/// Checks `latest.json` in the release directory.
pub(crate) fn verify(request: &Request<'_>) -> Outcome {
    let mut outcome = Outcome::default();
    let Some(version) = configured_version(request, &mut outcome) else {
        return outcome;
    };
    let Some(key) = configured_key(request.root, &mut outcome) else {
        return outcome;
    };
    let manifest = fs::read_to_string(request.directory.join(MANIFEST))
        .map_err(|error| error.to_string())
        .and_then(|text| serde_json::from_str::<Value>(&text).map_err(|error| error.to_string()));
    let manifest = match manifest {
        Ok(manifest) => manifest,
        Err(error) => {
            outcome.fail(MANIFEST.to_string(), 1, format!("cannot read it: {error}"));
            return outcome;
        }
    };
    if manifest.get("version").and_then(Value::as_str) != Some(version.as_str()) {
        outcome.fail(
            MANIFEST.to_string(),
            1,
            format!("`version` must be `{version}`, the version in {CONFIG}"),
        );
    }
    let platforms = manifest
        .get("platforms")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for required in REQUIRED {
        if !platforms.contains_key(required) {
            outcome.fail(
                MANIFEST.to_string(),
                1,
                format!("the platform `{required}` is missing"),
            );
        }
    }
    let entries = platforms.iter().collect::<BTreeMap<_, _>>();
    for (platform, entry) in &entries {
        check_entry(
            request.directory,
            &version,
            platform,
            entry,
            &key,
            &mut outcome,
        );
    }
    outcome.note(format!(
        "{} platform entries checked against version {version}",
        entries.len()
    ));
    outcome
}

/// One platform: its package exists, is signed by the key, and is in this release.
fn check_entry(
    directory: &Path,
    version: &str,
    platform: &str,
    entry: &Value,
    key: &PublicKey,
    outcome: &mut Outcome,
) {
    let mut fail = |message: String| outcome.fail(format!("{MANIFEST}:{platform}"), 1, message);
    let url = entry.get("url").and_then(Value::as_str).unwrap_or("");
    let signature = entry.get("signature").and_then(Value::as_str).unwrap_or("");
    let prefix = format!("{DOWNLOAD_BASE}/v{version}/");
    let Some(asset) = url.strip_prefix(&prefix).filter(|asset| plain_name(asset)) else {
        fail(format!(
            "the address `{url}` is not an asset of release v{version}; it must start with {prefix}"
        ));
        return;
    };
    let Ok(package) = fs::read(directory.join(asset)) else {
        fail(format!(
            "the asset `{asset}` is not in the release directory"
        ));
        return;
    };
    let on_disk = fs::read_to_string(directory.join(format!("{asset}.sig")));
    if on_disk.ok().as_deref().map(str::trim) != Some(signature.trim())
        || signature.trim().is_empty()
    {
        fail(format!(
            "the signature is not the contents of `{asset}.sig`"
        ));
        return;
    }
    let decoded = decode_base64_text(signature).and_then(|text| Signature::decode(&text).ok());
    let Some(decoded) = decoded else {
        fail("the signature is not a base64 minisign signature".to_string());
        return;
    };
    if key.verify(&package, &decoded, true).is_err() {
        fail(format!(
            "the signature does not verify `{asset}` under the public key in {CONFIG}"
        ));
        return;
    }
    let trusted = decoded.trusted_comment();
    let signed_version = trusted
        .split('\t')
        .find_map(|field| field.strip_prefix("version:"));
    if signed_version.is_some_and(|signed| signed != version) {
        fail(format!(
            "the signature was made for version {signed_version:?}, not {version}"
        ));
    }
}
