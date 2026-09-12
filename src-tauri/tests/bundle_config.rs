//! Guards on the packaging configuration.
//!
//! These catch the kind of mistake that only shows up during a release build on a
//! platform you are not sitting in front of.

use std::path::Path;

fn config() -> serde_json::Value {
    let raw = std::fs::read_to_string("tauri.conf.json").expect("read tauri.conf.json");
    serde_json::from_str(&raw).expect("tauri.conf.json must be valid JSON")
}

#[test]
fn versions_agree_across_the_manifests() {
    let conf = config();
    let conf_version = conf["version"].as_str().expect("a version in tauri.conf.json");

    let cargo = std::fs::read_to_string("Cargo.toml").unwrap();
    let cargo_version = cargo
        .lines()
        .find_map(|line| line.strip_prefix("version = "))
        .map(|value| value.trim().trim_matches('"'))
        .expect("a version in Cargo.toml");

    let package = std::fs::read_to_string("../package.json").unwrap();
    let package: serde_json::Value = serde_json::from_str(&package).unwrap();
    let package_version = package["version"].as_str().unwrap();

    // The release workflow builds from a tag; a mismatch here ships the wrong number.
    assert_eq!(conf_version, cargo_version, "tauri.conf.json vs Cargo.toml");
    assert_eq!(conf_version, package_version, "tauri.conf.json vs package.json");
}

#[test]
fn every_configured_icon_exists() {
    let conf = config();
    for icon in conf["bundle"]["icon"].as_array().expect("bundle.icon") {
        let path = icon.as_str().unwrap();
        assert!(
            Path::new(path).is_file(),
            "{path} is listed in bundle.icon but is not on disk"
        );
    }
}

#[test]
fn windows_and_macos_icons_are_the_right_formats() {
    // A .ico or .icns that is silently a PNG produces an installer with no icon.
    let ico = std::fs::read("icons/icon.ico").expect("icons/icon.ico");
    assert_eq!(&ico[..4], &[0x00, 0x00, 0x01, 0x00], "icon.ico is not an ICO file");

    let icns = std::fs::read("icons/icon.icns").expect("icons/icon.icns");
    assert_eq!(&icns[..4], b"icns", "icon.icns is not an ICNS file");
}

#[test]
fn the_macos_entitlements_file_referenced_by_the_config_exists() {
    let conf = config();
    let entitlements = conf["bundle"]["macOS"]["entitlements"]
        .as_str()
        .expect("bundle.macOS.entitlements");

    assert!(
        Path::new(entitlements).is_file(),
        "{entitlements} is referenced by the bundle config but is missing, which fails \
         the build only when signing on macOS"
    );
}

#[test]
fn the_identifier_is_not_the_tauri_default() {
    let conf = config();
    let identifier = conf["identifier"].as_str().unwrap();

    // `com.tauri.dev` collides with every other unconfigured Tauri app on the machine.
    assert_ne!(identifier, "com.tauri.dev");
    assert!(identifier.contains('.'), "{identifier} is not reverse-DNS");
}

#[test]
fn the_csp_is_set_and_does_not_allow_remote_code() {
    let conf = config();
    let csp = &conf["app"]["security"]["csp"];

    assert!(!csp.is_null(), "a null CSP disables the protection entirely");
    let script_src = csp["script-src"].as_str().expect("a script-src");
    assert_eq!(script_src, "'self'", "remote scripts must not be loadable");
}

#[test]
fn the_updater_polls_a_url_with_no_version_in_it() {
    let conf = config();
    let endpoints = conf["plugins"]["updater"]["endpoints"]
        .as_array()
        .expect("plugins.updater.endpoints");

    let endpoint = endpoints[0].as_str().unwrap();

    // Every already-installed copy asks this URL and only this URL. GitHub resolves
    // `latest/download` by filename, so the manifest keeps its name while the version
    // moves; changing the shape of this strands every copy already out there.
    assert!(
        endpoint.ends_with("/releases/latest/download/latest.json"),
        "{endpoint} is not the static manifest URL the release scripts publish"
    );
}

#[test]
fn updater_artifacts_are_left_to_the_release_config() {
    // `createUpdaterArtifacts` needs a signing key, and having it in the main config would
    // mean an ordinary unsigned `bun run tauri build` failed on a developer machine. The
    // release workflow and build-mac.sh turn it on with the overlay instead.
    let conf = config();
    assert!(
        conf["bundle"]["createUpdaterArtifacts"].is_null(),
        "the main config must not demand a signing key"
    );

    let raw = std::fs::read_to_string("tauri.updater.conf.json")
        .expect("src-tauri/tauri.updater.conf.json, passed as --config by the release build");
    let overlay: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(overlay["bundle"]["createUpdaterArtifacts"], serde_json::json!(true));
}

#[test]
fn bundle_targets_cover_every_supported_platform() {
    let conf = config();
    let targets: Vec<&str> = conf["bundle"]["targets"]
        .as_array()
        .expect("an explicit target list")
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();

    for expected in ["app", "dmg", "nsis", "deb", "appimage"] {
        assert!(targets.contains(&expected), "missing bundle target {expected}");
    }
}
