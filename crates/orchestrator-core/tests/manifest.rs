//! Round-trips the on-disk plugin manifest through the parser.
//!
//! Doubles as the CI manifest-check job — if `plugin/thurbox-plugin.toml`
//! ever drifts from the parser's expectations this fails.

use orchestrator_core::plugin::{Manifest, PLUGIN_API_VERSION};

const MANIFEST_SRC: &str = include_str!("../../../plugin/thurbox-plugin.toml");

#[test]
fn plugin_manifest_parses_and_matches_api_version() {
    let m = Manifest::from_toml(MANIFEST_SRC).expect("manifest parses");
    assert_eq!(m.name, "orchestrator");
    assert_eq!(m.thurbox_plugin_api, PLUGIN_API_VERSION);
    assert_eq!(m.process.exec, "bin/thurbox-plugin-orchestrator");
    assert!(m.process.capabilities.contains(&"mcp-tools".to_owned()));
    assert!(m
        .process
        .activation_events
        .contains(&"onStartupFinished".to_owned()));
}
