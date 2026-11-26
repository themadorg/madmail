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

    # --- destination type -------------------------------------------------
    # mv(1) moves its source *into* a directory destination, so a directory at
    # main.css produced exit 0, an install record, and a stray stylesheet left
    # inside the served tree, where any file is publicly fetchable.
    def test_refuses_a_directory_at_main_css(self):
        (self.www / "main.css").mkdir()
        result = switch(self.themes, self.www, PALETTE)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("not a regular file", result.stdout)
        self.assertEqual(list((self.www / "main.css").iterdir()), [])
        self.assertFalse((self.themes / ".installed").exists())

    # rename(2) replaces a destination symlink rather than following it, so
    # nothing was ever written outside the tree. Refusing avoids silently
    # destroying an arrangement an operator made on purpose.
    def test_refuses_a_symlink_at_main_css(self):
        outside = self.www.parent / "elsewhere.css"
        outside.write_text("untouched\n")
        (self.www / "main.css").symlink_to(outside)
        result = switch(self.themes, self.www, PALETTE)
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue((self.www / "main.css").is_symlink())
        self.assertEqual(outside.read_text(), "untouched\n")

    # -e is false for a dangling link, so only the -h test catches this one.
    def test_refuses_a_dangling_symlink_at_main_css(self):
        (self.www / "main.css").symlink_to(self.www.parent / "nothing.css")
        result = switch(self.themes, self.www, PALETTE)
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue((self.www / "main.css").is_symlink())

    # The guard stats rather than opens, so this does not block on a writer.
    def test_refuses_a_non_regular_file_at_main_css(self):
        os.mkfifo(self.www / "main.css")
        result = switch(self.themes, self.www, PALETTE)
        self.assertNotEqual(result.returncode, 0)

    def test_a_refused_install_leaves_no_temporary_files(self):
        (self.www / "main.css").mkdir()
        switch(self.themes, self.www, PALETTE)
        left = [q.name for q in self.www.rglob("*") if q.name.startswith(".main.css.")]
        self.assertEqual(left, [])

    def test_a_refused_install_does_not_change_the_record(self):
        switch(self.themes, self.www, PALETTE)
        before = (self.themes / ".installed").read_text()
        (self.www / "main.css").unlink()
        (self.www / "main.css").mkdir()
        result = switch(self.themes, self.www, "primer")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((self.themes / ".installed").read_text(), before)

    # --- palette names ----------------------------------------------------
    # known() asked whether a path resolved to a file, not whether the name was
    # one of ours, so a name carrying .. installed a stylesheet from outside
    # src/ and recorded it as the active palette.
    def test_rejects_a_palette_name_that_is_not_a_single_segment(self):
        (self.themes / "outside.main.css").write_text(":root { --x: 1; }\n")
        result = switch(self.themes, self.www, "../outside")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("palette names", result.stdout)
        self.assertFalse((self.www / "main.css").exists())
        self.assertFalse((self.themes / ".installed").exists())


if __name__ == "__main__":
    unittest.main()
