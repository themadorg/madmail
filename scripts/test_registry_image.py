"""Registry immutability checks. HTTP is mocked; nothing is pushed."""

import email.message
import io
import os
import tempfile
import unittest
import urllib.error
from pathlib import Path
from unittest import mock

import importlib.util
import sys

SPEC = importlib.util.spec_from_file_location(
    "madmail_registry", Path(__file__).with_name("registry_image.py")
)
registry = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = registry
SPEC.loader.exec_module(registry)

SHA = "a" * 40


def http_error(url, code):
    return urllib.error.HTTPError(
        url, code, "status", email.message.Message(), io.BytesIO(b"")
    )


class RegistryImageTests(unittest.TestCase):
    def test_only_http_404_means_the_tag_is_absent(self):
        def fake(url, headers):
            if "/token?" in url:
                return {"token": "t"}
            raise http_error(url, 404)

        with mock.patch.object(registry, "fetch", side_effect=fake):
            self.assertFalse(
                registry.image_exists("themadorg/madmail", "1.2.3-unstable", SHA)
            )

    def test_auth_and_server_errors_are_not_absence(self):
        def fake(url, headers):
            if "/token?" in url:
                return {"token": "t"}
            raise http_error(url, 401)

        with mock.patch.object(registry, "fetch", side_effect=fake):
            with self.assertRaises(urllib.error.HTTPError):
                registry.image_exists("themadorg/madmail", "1.2.3", SHA)

    def test_matching_revision_is_reused(self):
        def fake(url, headers):
            if "/token?" in url:
                return {"token": "t"}
            if url.endswith("/manifests/1.2.3-unstable"):
                return {
                    "manifests": [
                        {
                            "digest": "sha256:amd",
                            "platform": {"os": "linux", "architecture": "amd64"},
                        },
                        {
                            "digest": "sha256:arm",
                            "platform": {"os": "linux", "architecture": "arm64"},
                        },
                    ]
                }
            if url.endswith("/manifests/sha256:amd"):
                return {"config": {"digest": "sha256:cfg"}}
            if url.endswith("/blobs/sha256:cfg"):
                return {
                    "config": {
                        "Labels": {"org.opencontainers.image.revision": SHA}
                    }
                }
            raise AssertionError(url)

        with mock.patch.object(registry, "fetch", side_effect=fake):
            self.assertTrue(
                registry.image_exists("themadorg/madmail", "1.2.3-unstable", SHA)
            )

    def test_different_revision_conflicts(self):
        def fake(url, headers):
            if "/token?" in url:
                return {"token": "t"}
            if "/manifests/" in url:
                return {"config": {"digest": "sha256:cfg"}}
            return {"config": {"Labels": {"org.opencontainers.image.revision": "b" * 40}}}

        with mock.patch.object(registry, "fetch", side_effect=fake):
            with self.assertRaises(ValueError):
                registry.image_exists("themadorg/madmail", "1.2.3", SHA)

    def test_moving_tags_and_failed_lookups_do_not_report_absence(self):
        output = Path(self.enterContext(tempfile.TemporaryDirectory())) / "out"
        with mock.patch.dict(os.environ, {"GITHUB_OUTPUT": str(output)}):
            self.assertEqual(
                registry.main(
                    [
                        "--repository",
                        "themadorg/madmail",
                        "--tag",
                        "latest",
                        "--sha",
                        SHA,
                    ]
                ),
                1,
            )
            self.assertFalse(output.exists())
        output.write_text("")
        with (
            mock.patch.dict(os.environ, {"GITHUB_OUTPUT": str(output)}),
            mock.patch.object(registry, "image_exists", return_value=False),
        ):
            self.assertEqual(
                registry.main(
                    [
                        "--repository",
                        "themadorg/madmail",
                        "--tag",
                        "1.2.3-unstable",
                        "--sha",
                        SHA,
                    ]
                ),
                0,
            )
        self.assertEqual(output.read_text(), "exists=false\n")


if __name__ == "__main__":
    unittest.main()
