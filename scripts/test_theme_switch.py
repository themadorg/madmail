"""Theme switcher behaviour. Drives themes/switch against a temporary tree."""

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
SWITCH = REPO / "themes" / "switch"
PALETTE = "millennium"


def fixture(root):
    themes = root / "themes"
    themes.mkdir()
    shutil.copytree(REPO / "themes" / "src", themes / "src")
    www = root / "www"
    www.mkdir()
    return themes, www


def switch(themes, www, *args):
    env = dict(os.environ)
    env["MADMAIL_THEMES_DIR"] = str(themes)
    env["MADMAIL_WWW"] = str(www)
    return subprocess.run([str(SWITCH), *args], env=env, capture_output=True, text=True)


class ThemeSwitchTest(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.themes, self.www = fixture(Path(tmp.name))

    def test_lists_every_palette_in_src(self):
        out = switch(self.themes, self.www).stdout
        for path in (self.themes / "src").glob("*.main.css"):
            self.assertIn(path.name[: -len(".main.css")], out)

    def test_install_then_verify(self):
        self.assertEqual(switch(self.themes, self.www, PALETTE).returncode, 0)
        self.assertIn("verified", switch(self.themes, self.www).stdout)

    def test_default_mode_lands_directly_in_root(self):
        switch(self.themes, self.www, PALETTE, "--default-mode", "dark")
        lines = (self.www / "main.css").read_text().splitlines()
        at = lines.index(":root {")
        self.assertEqual(lines[at + 1].strip(), "--default-mode: dark;")
        self.assertIn("verified", switch(self.themes, self.www).stdout)

    def test_reinstalling_without_the_flag_clears_the_mode(self):
        switch(self.themes, self.www, PALETTE, "--default-mode", "dark")
        switch(self.themes, self.www, PALETTE)
        self.assertNotIn("--default-mode", (self.www / "main.css").read_text())
        self.assertIn("verified", switch(self.themes, self.www).stdout)

    def test_an_edited_stylesheet_is_a_palette_difference(self):
        switch(self.themes, self.www, PALETTE)
        with (self.www / "main.css").open("a") as fh:
            fh.write("/* local edit */\n")
        self.assertIn("main.css differs", switch(self.themes, self.www).stdout)

    def test_a_mode_difference_is_not_a_palette_difference(self):
        switch(self.themes, self.www, PALETTE, "--default-mode", "dark")
        (self.themes / ".installed").write_text(PALETTE + " light\n")
        out = switch(self.themes, self.www).stdout
        self.assertIn("palette matches", out)
        self.assertIn("default-mode is dark", out)

    def test_unknown_palette_fails(self):
        result = switch(self.themes, self.www, "nope")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("no such palette", result.stdout)

    def test_refuses_a_palette_with_unfilled_placeholders(self):
        (self.themes / "src" / "broken.main.css").write_text(
            ":root {\n  --background: __PLACEHOLDER__;\n}\n"
        )
        result = switch(self.themes, self.www, "broken")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unfilled placeholders", result.stdout)
        self.assertFalse((self.www / "main.css").exists())

    def test_refuses_a_palette_that_declares_default_mode(self):
        (self.themes / "src" / "pinned.main.css").write_text(
            ":root {\n  --default-mode: dark;\n}\n"
        )
        result = switch(self.themes, self.www, "pinned")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("declares --default-mode itself", result.stdout)

    def test_install_leaves_no_temporary_files(self):
        switch(self.themes, self.www, PALETTE)
        left = [p.name for p in self.www.iterdir() if p.name.startswith(".main.css.")]
        self.assertEqual(left, [])


if __name__ == "__main__":
    unittest.main()
