"""Packaging boundaries, not claims of native installer/platform qualification."""
import json
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
DESKTOP = ROOT / "src/onebrain-desktop"


class DesktopPackagingTests(unittest.TestCase):
    def test_obp_is_opt_in_and_forwards_both_shared_crates(self):
        cargo = tomllib.loads((DESKTOP / "Cargo.toml").read_text(encoding="utf-8"))
        self.assertEqual(cargo["features"]["default"], [])
        self.assertIn("onebrain-node/vnext-outbound-first", cargo["features"]["vnext-outbound-first"])
        self.assertIn("onebrain-api/vnext-outbound-first", cargo["features"]["vnext-outbound-first"])

    def test_packaged_web_and_single_tray_have_local_csp(self):
        config = json.loads((DESKTOP / "tauri.conf.json").read_text(encoding="utf-8"))
        self.assertEqual(config["build"]["frontendDist"], "../onebrain-web/dist")
        self.assertEqual(config["build"]["beforeBuildCommand"], "npm run build --prefix onebrain-web")
        self.assertNotIn("trayIcon", config["app"])
        csp = config["app"]["security"]["csp"]
        self.assertIn("ipc:", csp)
        self.assertIn("http://127.0.0.1:*", csp)
        self.assertIn("frame-src 'none'", csp)
        self.assertNotIn("connect-src *", csp)

    def test_capabilities_have_no_remote_webview_or_shell_execution(self):
        caps = json.loads((DESKTOP / "capabilities/default.json").read_text(encoding="utf-8"))
        self.assertEqual(caps["windows"], ["main"])
        self.assertNotIn("remote", caps)
        self.assertNotIn("shell:allow-execute", caps["permissions"])

    def test_explicit_restart_remains_after_shared_shutdown(self):
        commands = (DESKTOP / "src/commands.rs").read_text(encoding="utf-8")
        exit_body = commands.split("pub(crate) async fn finish_exit", 1)[1].split("fn local_main", 1)[0]
        self.assertLess(exit_body.index("supervisor.shutdown().await"), exit_body.index("app.restart()"))
        self.assertLess(exit_body.index("supervisor.shutdown().await"), exit_body.index("app.exit(0)"))


if __name__ == "__main__":
    unittest.main()
