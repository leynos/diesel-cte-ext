"""Contract test for the development build footprint rule.

The Concordat ``rust-dev-build-footprint`` rule needs the default development
profile to disable incremental compilation and debug information. The pinned
toolchain is stable, so the native Cargo metadata key does not apply and the
Cargo configuration must not carry an ``[unstable]`` metadata setting, which
stable Cargo cannot honour. This test holds both facts, then proves them at the
Cargo boundary: it builds a one-file crate that carries this repository's own
``[profile.dev]`` table and Cargo configuration under the pinned channel and
reads the rustc command line Cargo prints with ``-v``.

Run via ``make test-workflow-contracts``.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import tomllib
import typing as typ
from pathlib import Path

ROOT: typ.Final[Path] = Path(__file__).resolve().parents[2]
FIXTURE_LIB: typ.Final[str] = "pub fn footprint() {}\n"
RUSTC_LINE: typ.Final[re.Pattern[str]] = re.compile(r"Running `.*--crate-name footprint .*`")
WRAPPER_VARIABLES: typ.Final[tuple[str, ...]] = (
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "CARGO_BUILD_RUSTC_WRAPPER",
    "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
    "CARGO_ENCODED_RUSTFLAGS",
    "CARGO_BUILD_RUSTFLAGS",
    "RUSTFLAGS",
    "CARGO_INCREMENTAL",
    "CARGO_PROFILE_DEV_DEBUG",
    "CARGO_PROFILE_DEV_INCREMENTAL",
)


def _load(relative: str) -> dict[str, typ.Any]:
    """Return a parsed TOML file of the repository."""
    return tomllib.loads((ROOT / relative).read_text("utf-8"))


def _is_zero_debug(debug: object) -> bool:
    """Accept the exact integer ``0`` or the boolean ``false``.

    Python equality would also accept ``0.0``, which Cargo rejects.
    """
    is_zero_integer = (
        isinstance(debug, int) and not isinstance(debug, bool) and debug == 0
    )
    return debug is False or is_zero_integer


def test_development_profile_keeps_the_build_footprint_settings() -> None:
    """The default dev profile sets ``incremental = false`` and ``debug = 0``."""
    dev = _load("Cargo.toml").get("profile", {}).get("dev", {})
    assert dev.get("incremental") is False, "profile.dev must set incremental = false"
    assert _is_zero_debug(dev.get("debug")), "profile.dev must set debug = 0 or false"


def test_a_stable_pin_carries_no_unstable_metadata_setting() -> None:
    """A stable pin cannot honour ``[unstable]`` metadata keys, so none is set."""
    channel = _load("rust-toolchain.toml")["toolchain"]["channel"]
    assert not channel.startswith("nightly"), f"expected a stable pin, found {channel}"
    config_path = ROOT / ".cargo" / "config.toml"
    if config_path.exists():
        unstable = tomllib.loads(config_path.read_text("utf-8")).get("unstable", {})
        assert "embed-metadata" not in unstable, "stable Cargo cannot read embed-metadata"
        assert "no-embed-metadata" not in unstable, (
            "stable Cargo cannot read no-embed-metadata"
        )


def _dev_profile_text() -> str:
    """Return the repository's ``[profile.dev]`` table text, header included."""
    manifest = (ROOT / "Cargo.toml").read_text("utf-8")
    match = re.search(r"^\[profile\.dev\]\n(?:(?!\[).*\n?)*", manifest, re.MULTILINE)
    assert match is not None, "Cargo.toml must carry a [profile.dev] table"
    return match.group(0)


def _compile_fixture(
    tmp_path: Path, extra_env: dict[str, str] | None = None
) -> str:
    """Build the fixture crate and return the rustc command line Cargo ran.

    The fixture holds the repository's Cargo configuration and ``[profile.dev]``
    table, and builds under the repository's pinned channel with every
    wrapper and profile override removed from the environment, so only the
    repository's own settings and the override under test apply.
    """
    channel = _load("rust-toolchain.toml")["toolchain"]["channel"]
    crate = tmp_path / "footprint"
    (crate / "src").mkdir(parents=True)
    (crate / ".cargo").mkdir()
    (crate / "Cargo.toml").write_text(
        '[package]\nname = "footprint"\nversion = "0.0.0"\nedition = "2024"\n\n'
        + _dev_profile_text(),
        "utf-8",
    )
    (crate / "src" / "lib.rs").write_text(FIXTURE_LIB, "utf-8")
    config_path = ROOT / ".cargo" / "config.toml"
    if config_path.exists():
        shutil.copy(config_path, crate / ".cargo" / "config.toml")
    env = {k: v for k, v in os.environ.items() if k not in WRAPPER_VARIABLES}
    env["CARGO_TARGET_DIR"] = str(tmp_path / "target")
    env.update(extra_env or {})
    cargo = shutil.which("cargo")
    assert cargo is not None, "the contract suite requires Cargo"
    result = subprocess.run(  # noqa: S603 - a fixed command
        [cargo, f"+{channel}", "build", "--color", "never", "--offline", "-v"],
        cwd=crate,
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 0, f"cargo build failed:\n{result.stderr}"
    lines = RUSTC_LINE.findall(result.stderr)
    assert lines, f"no rustc command line in Cargo's output:\n{result.stderr}"
    return lines[0]


def test_cargo_builds_by_default_without_incremental_state_or_debug_info(
    tmp_path: Path,
) -> None:
    """A default dev build passes neither ``debuginfo`` nor ``incremental``."""
    command = _compile_fixture(tmp_path)
    assert "-C debuginfo" not in command, f"debug info must be off: {command}"
    assert "-C incremental" not in command, f"incremental must be off: {command}"


def test_the_documented_debug_override_turns_debug_info_back_on(
    tmp_path: Path,
) -> None:
    """``CARGO_PROFILE_DEV_DEBUG=true`` restores debug information."""
    command = _compile_fixture(tmp_path, {"CARGO_PROFILE_DEV_DEBUG": "true"})
    assert "-C debuginfo=2" in command, f"the override must restore debug info: {command}"


def test_the_documented_incremental_override_turns_incremental_back_on(
    tmp_path: Path,
) -> None:
    """``CARGO_INCREMENTAL=1`` restores incremental compilation."""
    command = _compile_fixture(tmp_path, {"CARGO_INCREMENTAL": "1"})
    assert "-C incremental=" in command, f"the override must restore incremental: {command}"
