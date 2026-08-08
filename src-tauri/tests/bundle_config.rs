use std::{fs, path::Path};

#[test]
fn bundle_explicitly_packages_the_application_logo() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = manifest_dir.join("tauri.conf.json");
    let config: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&config_path).expect("tauri.conf.json should be readable"),
    )
    .expect("tauri.conf.json should contain valid JSON");
    let configured_icons = config["bundle"]["icon"]
        .as_array()
        .expect("bundle.icon should explicitly list application logo assets")
        .iter()
        .map(|value| value.as_str().expect("bundle icon paths should be strings"))
        .collect::<Vec<_>>();
    let expected_icons = [
        "icons/32x32.png",
        "icons/128x128.png",
        "icons/128x128@2x.png",
        "icons/icon.icns",
        "icons/icon.ico",
    ];

    assert_eq!(configured_icons, expected_icons);
    for relative_path in expected_icons {
        assert!(
            manifest_dir.join(relative_path).is_file(),
            "configured bundle icon is missing: {relative_path}"
        );
    }
}
