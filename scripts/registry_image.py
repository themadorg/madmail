#!/usr/bin/env python3
"""Check GHCR immutable image existence/source; only HTTP 404 permits a build."""

import argparse
import base64
import json
import os
import re
import urllib.error
import urllib.parse
import urllib.request


def fetch(url, headers):
    with urllib.request.urlopen(
        urllib.request.Request(url, headers=headers), timeout=30
    ) as response:
        return json.load(response)


def image_exists(repository, tag, source_sha):
    if not re.fullmatch(r"[a-z0-9._-]+/[a-z0-9._-]+", repository) or not re.fullmatch(
        r"[0-9]+\.[0-9]+\.[0-9]+(?:-unstable)?", tag
    ):
        raise ValueError("Invalid registry repository/tag")
    headers = {}
    token = os.environ.get("GITHUB_TOKEN", "")
    if token:
        basic = base64.b64encode(
            (os.environ["GITHUB_ACTOR"] + ":" + token).encode()
        ).decode()
        headers["Authorization"] = "Basic " + basic
    auth = fetch(
        "https://ghcr.io/token?"
        + urllib.parse.urlencode(
            {"service": "ghcr.io", "scope": f"repository:{repository}:pull"}
        ),
        headers,
    )
    bearer = auth.get("token") or auth["access_token"]
    headers = {
        "Authorization": "Bearer " + bearer,
        "Accept": "application/vnd.oci.image.index.v1+json, application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.list.v2+json, application/vnd.docker.distribution.manifest.v2+json",
    }
    base = f"https://ghcr.io/v2/{repository}/"
    try:
        manifest = fetch(base + "manifests/" + tag, headers)
    except urllib.error.HTTPError as exc:
        if exc.code == 404:
            return False
        raise
    if "manifests" in manifest:
        images = [
            m
            for m in manifest["manifests"]
            if m.get("platform", {}).get("os") == "linux"
            and m.get("platform", {}).get("architecture") == "amd64"
        ]
        if len(images) != 1:
            raise ValueError("Expected one Linux amd64 image")
        manifest = fetch(base + "manifests/" + images[0]["digest"], headers)
    config = fetch(base + "blobs/" + manifest["config"]["digest"], headers)
    if (
        config["config"].get("Labels", {}).get("org.opencontainers.image.revision")
        != source_sha
    ):
        raise ValueError("Immutable image conflicts with selected source revision")
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--sha", required=True)
    args = parser.parse_args()
    try:
        exists = image_exists(args.repository, args.tag, args.sha)
        with open(os.environ["GITHUB_OUTPUT"], "a") as output:
            output.write(f"exists={str(exists).lower()}\n")
    except (OSError, ValueError, KeyError, TypeError):
        # Auth/connection/config failures must never be mistaken for an absent tag.
        print(
            "Registry validation failed; refusing to create or replace an immutable image."
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
