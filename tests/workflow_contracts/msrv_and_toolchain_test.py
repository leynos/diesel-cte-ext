"""Contract tests for the MSRV job and the toolchain Dependabot entry.

The ``msrv`` job in ``ci.yml`` is declarative configuration around one shell
script, and the ``rust-toolchain`` Dependabot entry is declarative
configuration only. These tests parse both with PyYAML, run the job's script
against stub ``rustup`` and ``cargo`` commands to check what it asks them to
do, and pin the settings whose loss would fail CI only on a later pull request
(the mold linker missing, a stored token, an empty MSRV).

Run via ``make test-workflow-contracts``.
"""

from __future__ import annotations

import os
import stat
import subprocess
import tomllib
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
CI_PATH = ROOT / ".github" / "workflows" / "ci.yml"
DEPENDABOT_PATH = ROOT / ".github" / "dependabot.yml"
CARGO_PATH = ROOT / "Cargo.toml"
STEP_NAME = "Check the library on the MSRV"

_STUB = """#!/bin/sh
printf '%s %s\\n' "$(basename "$0")" "$*" >> "$STUB_LOG"
"""


def _msrv_job() -> dict[str, object]:
    """Return the ``msrv`` job of the CI workflow."""
    workflow = yaml.safe_load(CI_PATH.read_text(encoding="utf-8"))
    job = workflow["jobs"]["msrv"]
    assert isinstance(job, dict), "ci.yml must define an msrv job"
    return job


def _script() -> str:
    """Return the shell script of the MSRV check step."""
    steps = _msrv_job()["steps"]
    matching = [s for s in steps if s.get("name") == STEP_NAME]
    assert len(matching) == 1, f"expected one step named {STEP_NAME!r}"
    return matching[0]["run"]


def _run(tmp_path: Path, manifest: str) -> tuple[int, list[str]]:
    """Run the step's script over *manifest* with stub rustup and cargo."""
    (tmp_path / "Cargo.toml").write_text(manifest, encoding="utf-8")
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    for name in ("rustup", "cargo"):
        stub = bin_dir / name
        stub.write_text(_STUB, encoding="utf-8")
        stub.chmod(stub.stat().st_mode | stat.S_IXUSR)
    log = tmp_path / "calls.log"
    log.touch()
    env = {
        **os.environ,
        "PATH": f"{bin_dir}{os.pathsep}{os.environ['PATH']}",
        "STUB_LOG": str(log),
    }
    result = subprocess.run(  # noqa: S603 - fixed argv, a test-controlled script
        ["bash", "-euo", "pipefail", "-c", _script()],  # noqa: S607
        cwd=tmp_path,
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    return result.returncode, log.read_text(encoding="utf-8").splitlines()


def test_the_job_installs_and_checks_the_declared_msrv(tmp_path: Path) -> None:
    """The toolchain comes from Cargo.toml and the check is one locked lib check."""
    code, calls = _run(
        tmp_path, '[package]\nname = "x"\nrust-version = "1.86"\nedition = "2024"\n'
    )

    assert code == 0, calls
    assert calls == [
        "rustup toolchain install 1.86 --profile minimal --no-self-update",
        "cargo +1.86 check --lib --locked --all-features",
    ]


def test_an_indented_workspace_rust_version_is_found(tmp_path: Path) -> None:
    """A `rust-version` key under any table is read, indented or not."""
    code, calls = _run(
        tmp_path, '[workspace.package]\n  rust-version = "1.87"\n'
    )

    assert code == 0, calls
    assert calls[-1] == "cargo +1.87 check --lib --locked --all-features"


def test_a_manifest_without_rust_version_fails_before_any_toolchain_call(
    tmp_path: Path,
) -> None:
    """A missing MSRV is an explicit failure, not an empty toolchain name."""
    code, calls = _run(tmp_path, '[package]\nname = "x"\n')

    assert code != 0, calls
    assert calls == [], calls


def test_the_job_runs_without_the_repository_rustflags_and_keeps_no_token() -> None:
    """RUSTFLAGS is assigned empty (no mold) and the checkout stores no credential."""
    job = _msrv_job()

    assert job["env"]["RUSTFLAGS"] == "", job["env"]
    checkout = job["steps"][0]
    assert checkout["uses"].startswith("actions/checkout@"), checkout
    assert checkout["with"]["persist-credentials"] is False, checkout
    assert job["permissions"] == {"contents": "read"}, job["permissions"]


def test_the_manifest_declares_the_rust_version() -> None:
    """Cargo.toml carries a rust-version, which the job reads."""
    manifest = tomllib.loads(CARGO_PATH.read_text(encoding="utf-8"))

    assert manifest["package"]["rust-version"] == "1.86", manifest["package"]


def test_dependabot_moves_the_toolchain_pin() -> None:
    """A daily, grouped `rust-toolchain` entry covers the root pin."""
    config = yaml.safe_load(DEPENDABOT_PATH.read_text(encoding="utf-8"))
    entries = [
        e for e in config["updates"] if e["package-ecosystem"] == "rust-toolchain"
    ]

    assert len(entries) == 1, config["updates"]
    entry = entries[0]
    assert entry["directory"] == "/", entry
    assert entry["schedule"]["interval"] == "daily", entry
    group = entry["groups"]["minor-and-patch"]
    assert group["patterns"] == ["*"], group
    assert sorted(group["update-types"]) == ["minor", "patch"], group
    assert entry["labels"] == ["dependencies", "rust-toolchain"], entry
    assert entry["open-pull-requests-limit"] == 5, entry
    assert entry["cooldown"] == {"default-days": 7}, entry
