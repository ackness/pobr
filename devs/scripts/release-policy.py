#!/usr/bin/env python3
"""Validate release metadata without building or writing to origin (Python 3.11+).

Successful commands emit one boolean key to stdout and, if set, GITHUB_OUTPUT.
Diagnostics go to stderr; invalid metadata and failed remote lookups are fatal.
The checkout is the workspace containing this script, independent of cwd.

Deployment assumes immutable tags and permits only the highest stable v0.x.y
tag currently on origin. A newer stable tag with failing CI intentionally blocks
older deployments; callers must serialize deployments separately. Build metadata
does not affect SemVer precedence and is excluded from production tags.
"""

import argparse
from dataclasses import dataclass
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib


ROOT = Path(__file__).resolve().parents[2]
SEMVER = re.compile(
    r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    r"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
)
OBJECT_ID = re.compile(r"(?:[0-9a-fA-F]{40}|[0-9a-fA-F]{64})")


class PolicyError(Exception):
    pass


@dataclass(frozen=True)
class Version:
    major: int
    minor: int
    patch: int
    prerelease: tuple[str, ...]
    build: str | None

    @classmethod
    def parse(cls, value):
        match = SEMVER.fullmatch(value) if isinstance(value, str) else None
        if match is None:
            raise PolicyError(f"invalid SemVer: {value!r}")
        major, minor, patch, prerelease, build = match.groups()
        identifiers = tuple(prerelease.split(".")) if prerelease else ()
        if any(part.isdigit() and len(part) > 1 and part[0] == "0"
               for part in identifiers):
            raise PolicyError(f"numeric prerelease identifier has a leading zero: {value!r}")
        return cls(int(major), int(minor), int(patch), identifiers, build)

    def precedence(self):
        # Numeric identifiers sort numerically, before nonnumeric identifiers.
        pre = tuple((0, int(part)) if part.isdigit() else (1, part)
                    for part in self.prerelease)
        return self.major, self.minor, self.patch, not self.prerelease, pre

    def is_production(self):
        return self.major == 0 and not self.prerelease and self.build is None


def tag_version(tag):
    if not tag.startswith("v"):
        raise PolicyError(f"expected a canonical v<SemVer> tag: {tag!r}")
    return Version.parse(tag[1:])


def git(*args):
    result = subprocess.run(
        ["git", *args], cwd=ROOT, capture_output=True, text=True, check=False,
    )
    if result.returncode:
        raise PolicyError(f"git {args[0]} failed ({result.returncode}): {result.stderr.strip()}")
    return result.stdout.strip()


def commit(revision):
    return git("rev-parse", "--verify", "--end-of-options", f"{revision}^{{commit}}")


def expected_sha(value):
    if not OBJECT_ID.fullmatch(value):
        raise PolicyError("expected a full commit SHA (40 or 64 hex digits)")
    return value.lower()


def read_toml(path):
    with path.open("rb") as source:
        return tomllib.load(source)


def workspace_version(manifest):
    try:
        value = manifest["workspace"]["package"]["version"]
    except (KeyError, TypeError) as error:
        raise PolicyError("missing [workspace.package].version") from error
    if not isinstance(value, str) or not value:
        raise PolicyError("[workspace.package].version must be a nonempty string")
    return value


def check_lock(manifest, version):
    workspace = manifest["workspace"]
    excluded = {path.resolve() for pattern in workspace.get("exclude", [])
                for path in ROOT.glob(pattern)}
    members = {ROOT} if "package" in manifest else set()
    for pattern in workspace.get("members", []):
        matches = list(ROOT.glob(pattern))
        if not matches:
            raise PolicyError(f"workspace member does not exist: {pattern}")
        members.update(path.resolve() for path in matches)
    locked = read_toml(ROOT / "Cargo.lock").get("package", [])
    for member in sorted(members - excluded):
        package = read_toml(member / "Cargo.toml")["package"]
        inherited = package.get("version")
        if not isinstance(inherited, dict) or inherited.get("workspace") is not True:
            continue
        name = package["name"]
        versions = [entry.get("version") for entry in locked
                    if entry.get("name") == name and "source" not in entry]
        if versions != [version]:
            raise PolicyError(
                f"Cargo.lock has {name} versions {versions!r}, expected [{version!r}]; "
                "refresh the lockfile"
            )


def remote_tags(*patterns):
    # No --exit-code: an empty successful lookup means absent, not a failure.
    output = git("ls-remote", "--tags", "origin", *patterns)
    refs = {}
    for line in output.splitlines():
        fields = line.split()
        if (len(fields) != 2 or not OBJECT_ID.fullmatch(fields[0])
                or not fields[1].startswith("refs/tags/")):
            raise PolicyError(f"invalid git ls-remote output: {line!r}")
        sha, ref = fields
        if ref in refs and refs[ref] != sha.lower():
            raise PolicyError(f"conflicting remote tag: {ref}")
        refs[ref] = sha.lower()
    for ref in refs:
        if ref.endswith("^{}") and ref[:-3] not in refs:
            raise PolicyError(f"remote peeled tag has no tag ref: {ref}")
    return {ref.removeprefix("refs/tags/"): refs.get(ref + "^{}", sha)
            for ref, sha in refs.items() if not ref.endswith("^{}")}


def bump(base, allow_existing_tag=False, detect_only=False):
    manifest = read_toml(ROOT / "Cargo.toml")
    new = workspace_version(manifest)
    old = workspace_version(tomllib.loads(git("show", f"{commit(base)}:Cargo.toml")))
    if detect_only:
        return "release", new != old
    current, previous = Version.parse(new), Version.parse(old)
    if new == old:
        return "release", False
    if current.precedence() <= previous.precedence():
        raise PolicyError(f"workspace version must increase in SemVer precedence: {old} -> {new}")
    check_lock(manifest, new)
    tag = f"v{new}"
    tags = remote_tags(f"refs/tags/{tag}", f"refs/tags/{tag}^{{}}")
    if tag in tags and not (allow_existing_tag and tags[tag] == commit("HEAD")):
        raise PolicyError(f"tag {tag} already exists (only a matching HEAD retry is allowed)")
    return "release", True


def check_tag(tag, sha):
    version = tag_version(tag)
    sha = expected_sha(sha)
    manifest = read_toml(ROOT / "Cargo.toml")
    current = workspace_version(manifest)
    if tag != f"v{current}":
        raise PolicyError(f"tag {tag} does not match workspace version {current}")
    check_lock(manifest, current)
    if commit(f"refs/tags/{tag}") != sha:
        raise PolicyError(f"tag {tag} does not point to expected commit {sha}")
    if commit("HEAD") != sha:
        raise PolicyError(f"checkout HEAD does not match expected commit {sha}")
    return "production", version.is_production()


def deployment(tag, sha):
    version = tag_version(tag)
    sha = expected_sha(sha)
    tags = remote_tags("refs/tags/v*")
    if tags.get(tag) != sha:
        raise PolicyError(f"remote tag {tag} is missing or moved from expected commit {sha}")
    stable = []
    for candidate in tags:
        try:
            candidate_version = tag_version(candidate)
        except PolicyError:
            continue
        if candidate_version.is_production():
            stable.append((candidate_version.precedence(), candidate))
    latest = max(stable)[1] if stable else None
    return "deploy", version.is_production() and tag == latest


def emit(key, value):
    line = f"{key}={str(value).lower()}\n"
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
            output.write(line)
    print(line, end="")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    bump_parser = commands.add_parser("bump", help="validate a workspace version change")
    bump_parser.add_argument("base")
    bump_parser.add_argument("--allow-existing-tag", action="store_true")
    detect_parser = commands.add_parser("detect", help="detect a version change without validation or network")
    detect_parser.add_argument("base")
    for command in ("tag", "deployment"):
        subparser = commands.add_parser(command)
        subparser.add_argument("tag")
        subparser.add_argument("sha", help="full expected commit SHA")
    args = parser.parse_args(argv)
    try:
        if args.command == "bump":
            result = bump(args.base, args.allow_existing_tag)
        elif args.command == "detect":
            result = bump(args.base, detect_only=True)
        elif args.command == "tag":
            result = check_tag(args.tag, args.sha)
        else:
            result = deployment(args.tag, args.sha)
        emit(*result)
    except (PolicyError, OSError, ValueError, KeyError, TypeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
