"""Load only the three pinned RePoE files needed to verify source envelopes.

The verified byte snapshots are compiled directly. Imports never reread mutable
code files or fall back to an installed RePoE package or an acquisition module.
This build-time boundary does not approve sources or adopt runtime data.
"""
from contextlib import contextmanager
import copy
import importlib.machinery
import os
from pathlib import Path
import re
import stat
import sys
import types
import uuid

from pipeline.imports.data_imports import acquire, pinned, require, validate_pin, write


FILES = ("RePoE/__init__.py", "RePoE/source_io.py", "RePoE/source_rebinding.py")
MAX_CODE_BYTES = 256 * 1024


def validate(spec):
    require(isinstance(spec, dict) and set(spec) == {"schema", "repository", "revision", "files"}
            and spec["schema"] == "pobr-source-verifier/v1"
            and spec["repository"] == "ackness/repoe"
            and isinstance(spec["revision"], str) and re.fullmatch(r"[a-f0-9]{40}", spec["revision"]),
            "Invalid pinned RePoE verifier identity")
    require(isinstance(spec["files"], dict) and set(spec["files"]) == set(FILES),
            "RePoE verifier requires its exact three-file closure")
    for identity in spec["files"].values():
        require(isinstance(identity, dict) and set(identity) == {"bytes", "sha256"},
                "Unknown RePoE verifier file-pin fields")
        validate_pin(identity)
        require(0 < identity["bytes"] <= MAX_CODE_BYTES, "RePoE verifier code exceeds its bound")


def _safe(path):
    path = Path(os.path.abspath(path))
    require(all(not item.is_symlink() for item in (path, *path.parents)), "Symlink verifier path")
    return path


def materialize(spec, output, cache, offline, fetcher):
    """Acquire fixed Git blobs without importing them; partial copies are not used."""
    validate(spec)
    output = _safe(output)
    require(not output.exists(), "Verifier output already exists")
    output.mkdir(parents=True)
    for name in FILES:
        raw = acquire(spec, name, spec["files"][name], cache, offline, fetcher)
        write(output / name, raw)
    _payloads(output, spec)
    return output


def _payloads(root, spec):
    root = _safe(root)
    require(root.is_dir(), "Missing RePoE verifier directory")
    found = set()
    for child in root.rglob("*"):
        _safe(child)
        name = child.relative_to(root).as_posix()
        if child.is_dir():
            require(name == "RePoE", "Unexpected verifier directory")
        else:
            require(child.is_file() and name in FILES, "Unexpected verifier file")
            found.add(name)
    require(found == set(FILES), "Incomplete RePoE verifier closure")
    result = {}
    for name in FILES:
        identity = spec["files"][name]
        flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0)
        with os.fdopen(os.open(root / name, flags), "rb") as stream:
            info = os.fstat(stream.fileno())
            require(stat.S_ISREG(info.st_mode) and info.st_size == identity["bytes"],
                    "Verifier source size/type mismatch")
            result[name] = pinned(stream.read(identity["bytes"] + 1), identity)
    return result


@contextmanager
def open_verifier(root, spec):
    """Yield (verify callable, provenance); pin every file before any execution."""
    validate(spec)
    spec = copy.deepcopy(spec)
    root = _safe(root)
    payloads = _payloads(root, spec)
    prefix = "_pobr_source_verifier_" + uuid.uuid4().hex
    try:
        package = types.ModuleType(prefix)
        package.__package__ = prefix
        package.__path__ = []  # No unpinned filesystem import fallback.
        package.__spec__ = importlib.machinery.ModuleSpec(prefix, loader=None, is_package=True)
        package.__file__ = str(root / FILES[0])
        sys.modules[prefix] = package
        modules = [(prefix, package, FILES[0])]
        for name in FILES[1:]:
            stem = Path(name).stem
            module = types.ModuleType(prefix + "." + stem)
            module.__package__ = prefix
            module.__file__ = str(root / name)
            module.__spec__ = importlib.machinery.ModuleSpec(module.__name__, loader=None)
            sys.modules[module.__name__] = module
            setattr(package, stem, module)
            modules.append((module.__name__, module, name))
        try:
            for _, module, name in modules:
                exec(compile(payloads[name], str(root / name), "exec"), module.__dict__)
            verify = modules[-1][1].verify
            require(callable(verify), "Pinned RePoE verifier has no verify API")
        except Exception as error:
            raise ValueError("Pinned RePoE verifier initialization failed") from error
        yield verify, {"schema": "pobr-source-verifier-use/v1", "library": copy.deepcopy(spec)}
        require(_payloads(root, spec) == payloads, "Pinned RePoE verifier changed during use")
    finally:
        for name in list(sys.modules):
            if name == prefix or name.startswith(prefix + "."):
                del sys.modules[name]
