#!/usr/bin/env python3

from __future__ import annotations

import argparse
import datetime
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Optional

# Enable ANSI escape sequences on Windows console
if sys.platform == "win32":
    try:
        os.system("")
    except Exception:
        pass


# --- ANSI Color Helpers ---
class Style:
    RESET = "\033[0m"
    BOLD = "\033[1m"
    DIM = "\033[2m"
    RED = "\033[31m"
    GREEN = "\033[32m"
    YELLOW = "\033[33m"
    BLUE = "\033[34m"
    CYAN = "\033[36m"


def supports_color() -> bool:
    if os.environ.get("NO_COLOR") or os.environ.get("TERM") == "dumb":
        return False
    return sys.stdout.isatty()


def info(msg: str) -> None:
    c = Style.BLUE + Style.BOLD if supports_color() else ""
    r = Style.RESET if supports_color() else ""
    print(f"{c}==>{r} {msg}")


def success(msg: str) -> None:
    c = Style.GREEN + Style.BOLD if supports_color() else ""
    r = Style.RESET if supports_color() else ""
    print(f"{c}✔{r} {msg}")


def warn(msg: str) -> None:
    c = Style.YELLOW + Style.BOLD if supports_color() else ""
    r = Style.RESET if supports_color() else ""
    print(f"{c}▲ WARNING:{r} {msg}")


def error(msg: str) -> None:
    c = Style.RED + Style.BOLD if supports_color() else ""
    r = Style.RESET if supports_color() else ""
    print(f"{c}✖ ERROR:{r} {msg}", file=sys.stderr)


def abort(msg: str, exit_code: int = 1) -> None:
    """Print an error message and exit."""
    error(msg)
    sys.exit(exit_code)


# --- Subprocess Execution Helper ---
def run_cmd(
    cmd: list[str],
    cwd: Optional[Path] = None,
    capture_output: bool = False,
    check: bool = True,
    env: Optional[dict[str, str]] = None,
) -> subprocess.CompletedProcess[str]:
    cwd_str = f" (in {cwd})" if cwd else ""
    dim = Style.DIM if supports_color() else ""
    reset = Style.RESET if supports_color() else ""
    print(f"{dim}  $ {' '.join(cmd)}{cwd_str}{reset}")

    merged_env = None
    if env is not None:
        merged_env = os.environ.copy()
        merged_env.update(env)

    try:
        return subprocess.run(
            cmd,
            cwd=cwd,
            text=True,
            capture_output=capture_output,
            check=check,
            env=merged_env,
        )
    except subprocess.CalledProcessError as e:
        if capture_output:
            if e.stdout:
                print(e.stdout, file=sys.stdout)
            if e.stderr:
                print(e.stderr, file=sys.stderr)
        raise RuntimeError(
            f"Command failed with exit code {e.returncode}: {' '.join(cmd)}"
        ) from e


def probe_command(cmd: list[str], cwd: Optional[Path] = None) -> tuple[bool, str]:
    """Runs a command silently and returns (success, output)."""
    try:
        res = subprocess.run(
            cmd,
            cwd=cwd,
            capture_output=True,
            text=True,
            check=False,
        )
        out = (res.stdout or res.stderr or "").strip()
        return res.returncode == 0, out
    except Exception as e:
        return False, str(e)


# --- Safe File Writing (Forces LF) ---
def write_file_lf(path: Path, content: str) -> None:
    """Writes text with strict LF (\\n) line endings across all platforms."""
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(content)


# --- Version Helpers ---
def parse_semver(version_str: str) -> tuple[int, int, int, str]:
    pattern = r"^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$"
    match = re.match(pattern, version_str.strip())
    if not match:
        raise ValueError(f"Invalid semantic version string: '{version_str}'")
    major, minor, patch, prerelease = match.groups()
    return int(major), int(minor), int(patch), (prerelease or "")


def bump_version(current: str, bump_type: str) -> str:
    major, minor, patch, _ = parse_semver(current)
    if bump_type == "patch":
        new_ver = f"{major}.{minor}.{patch + 1}"
    elif bump_type == "minor":
        new_ver = f"{major}.{minor + 1}.0"
    elif bump_type == "major":
        new_ver = f"{major + 1}.0.0"
    else:
        new_ver = bump_type
        new_maj, new_min, new_pat, _ = parse_semver(new_ver)
        if (new_maj, new_min, new_pat) <= (major, minor, patch):
            raise ValueError(
                f"New version '{new_ver}' must be strictly greater than "
                f"current version '{current}'"
            )
        return new_ver

    return new_ver


# --- Repository Helpers ---
def get_repo_root() -> Path:
    if not shutil.which("git"):
        abort("Required command 'git' is not installed or not found in PATH.")
    try:
        res = run_cmd(["git", "rev-parse", "--show-toplevel"], capture_output=True)
        return Path(res.stdout.strip())
    except Exception as e:
        abort(f"Not inside a git repository: {e}")


def read_current_version(root: Path) -> str:
    cargo_path = root / "Cargo.toml"
    content = cargo_path.read_text(encoding="utf-8")
    match = re.search(r'(?m)^\[package\][\s\S]*?^version\s*=\s*"([^"]+)"', content)
    if not match:
        raise ValueError("Could not find [package].version in root Cargo.toml")
    return match.group(1)


def get_git_remote(root: Path, branch: str) -> str:
    try:
        res = subprocess.run(
            ["git", "config", f"branch.{branch}.remote"],
            cwd=root,
            capture_output=True,
            text=True,
        )
        return res.stdout.strip() or "origin"
    except Exception:
        return "origin"


# --- Tool Resolution Helpers ---
def find_ruff_cmd() -> Optional[list[str]]:
    """Detects whether ruff is available via PATH or as a Python module."""
    if shutil.which("ruff"):
        return ["ruff"]
    try:
        res = subprocess.run(
            [sys.executable, "-m", "ruff", "--version"],
            capture_output=True,
            text=True,
        )
        if res.returncode == 0:
            return [sys.executable, "-m", "ruff"]
    except Exception:
        pass
    return None


def check_python_module(module_name: str) -> bool:
    """Checks whether a Python module is importable in the current interpreter."""
    try:
        res = subprocess.run(
            [sys.executable, "-c", f"import {module_name}"],
            capture_output=True,
        )
        return res.returncode == 0
    except Exception:
        return False


def verify_crates_io_auth() -> bool:
    """Checks if credentials exist for publishing to crates.io."""
    if os.environ.get("CARGO_REGISTRY_TOKEN"):
        return True
    home = Path.home()
    cred_toml = home / ".cargo" / "credentials.toml"
    cred_plain = home / ".cargo" / "credentials"
    for p in [cred_toml, cred_plain]:
        if p.exists() and "token" in p.read_text(encoding="utf-8", errors="ignore"):
            return True
    return False


# --- Pre-flight Tool Diagnostics ---
def run_tool_diagnostics(
    root: Path, skip_tests: bool, publish_cargo: bool
) -> list[str]:
    """Validates all required tools and runtime environments before execution."""
    info("Step 1: Running Pre-flight Environment & Tool Diagnostics")
    issues: list[str] = []

    # 1. Git
    git_ok, git_out = probe_command(["git", "--version"])
    if not git_ok:
        issues.append("Tool 'git' is not installed or not found in PATH.")
    else:
        print(f"  ✔ git:          {git_out}")

    # Check Git author details
    name_ok, user_name = probe_command(["git", "config", "user.name"])
    email_ok, user_email = probe_command(["git", "config", "user.email"])
    if not name_ok or not user_name or not email_ok or not user_email:
        issues.append(
            "Git user identity is incomplete. Set both 'git config user.name' "
            "and 'git config user.email' before tagging."
        )
    else:
        print(f"  ✔ git author:   {user_name} <{user_email}>")

    # Check for unfinished git operations (rebase, merge, cherry-pick)
    git_dir_ok, git_dir_str = probe_command(["git", "rev-parse", "--git-dir"], cwd=root)
    if git_dir_ok and git_dir_str:
        git_dir = Path(git_dir_str)
        if not git_dir.is_absolute():
            git_dir = root / git_dir
        for marker, op in [
            ("MERGE_HEAD", "merge"),
            ("REBASE_HEAD", "rebase"),
            ("rebase-merge", "rebase"),
            ("rebase-apply", "rebase"),
            ("CHERRY_PICK_HEAD", "cherry-pick"),
            ("REVERT_HEAD", "revert"),
        ]:
            if (git_dir / marker).exists():
                issues.append(
                    f"A git {op} operation is currently in progress. "
                    "Resolve or abort it before releasing."
                )

    # 2. Cargo & Rust Toolchain
    cargo_ok, cargo_out = probe_command(["cargo", "--version"])
    if not cargo_ok:
        issues.append("Tool 'cargo' is not installed or not found in PATH.")
    else:
        print(f"  ✔ cargo:        {cargo_out}")

    rustc_ok, rustc_out = probe_command(["rustc", "--version"])
    if not rustc_ok:
        issues.append("Tool 'rustc' is not installed or not found in PATH.")
    else:
        print(f"  ✔ rustc:        {rustc_out}")
        # Enforce PyO3 MSRV (Rust 1.83.0+)
        m = re.search(r"rustc\s+(\d+)\.(\d+)\.(\d+)", rustc_out)
        if m:
            maj, min_ver = int(m.group(1)), int(m.group(2))
            if (maj, min_ver) < (1, 83):
                issues.append(
                    f"Rust 1.83.0+ is required by mpatch and PyO3 (detected: {rustc_out}). "
                    "Update using 'rustup update'."
                )

    if not skip_tests:
        # Check rustfmt component
        fmt_ok, fmt_out = probe_command(["cargo", "fmt", "--version"])
        if not fmt_ok:
            issues.append(
                "Cargo component 'rustfmt' is not installed. "
                "Install with: rustup component add rustfmt"
            )
        else:
            print(f"  ✔ rustfmt:      {fmt_out}")

        # Check clippy component
        clippy_ok, clippy_out = probe_command(["cargo", "clippy", "--version"])
        if not clippy_ok:
            issues.append(
                "Cargo component 'clippy' is not installed. "
                "Install with: rustup component add clippy"
            )
        else:
            print(f"  ✔ clippy:       {clippy_out}")

    # 3. Python Environment
    py_ver = (
        f"{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}"
    )
    if sys.version_info < (3, 8):
        issues.append(f"Python 3.8+ is required (active interpreter: {py_ver}).")
    else:
        print(f"  ✔ python:       {py_ver} ({sys.executable})")

    # 4. Ruff (Python Linter / Formatter)
    ruff_cmd = find_ruff_cmd()
    if not skip_tests:
        if not ruff_cmd:
            issues.append(
                "Tool 'ruff' is not installed in PATH or the current Python environment.\n"
                "    ↳ Install with: pip install ruff (or pass --skip-tests)."
            )
        else:
            _, ruff_ver = probe_command(ruff_cmd + ["--version"])
            print(f"  ✔ ruff:         {ruff_ver} ({' '.join(ruff_cmd)})")

    # 5. Pytest & mpatch Module status
    has_pytest = check_python_module("pytest")
    has_mpatch = check_python_module("mpatch")
    if has_pytest and has_mpatch:
        _, pytest_ver = probe_command([sys.executable, "-m", "pytest", "--version"])
        print(f"  ✔ pytest:       {pytest_ver} (mpatch bindings detected)")
    elif has_pytest and not has_mpatch:
        print(
            "  ▲ pytest:       available, but 'mpatch' module is not installed in "
            "current environment"
        )
    else:
        print("  - pytest:       not installed in active Python environment (optional)")

    # 6. Crates.io authentication check (if cargo publish requested)
    if publish_cargo and not verify_crates_io_auth():
        warn(
            "No crates.io token detected in CARGO_REGISTRY_TOKEN or ~/.cargo/credentials[.toml]. "
            "'cargo publish' may prompt or fail if unauthenticated."
        )

    return issues


# --- Rollback Context ---
class ReleaseContext:
    def __init__(self, root: Path, dry_run: bool):
        self.root = root
        self.dry_run = dry_run
        self.modified_files: list[Path] = []
        self.committed = False

    def track(self, path: Path) -> None:
        if path not in self.modified_files:
            self.modified_files.append(path)

    def rollback(self) -> None:
        if self.dry_run or self.committed or not self.modified_files:
            return
        warn("Rolling back modified files to git HEAD state...")
        paths = [
            f.relative_to(self.root).as_posix()
            for f in self.modified_files
            if f.exists()
        ]
        if paths:
            subprocess.run(
                ["git", "reset", "HEAD", "--"] + paths,
                cwd=self.root,
                check=False,
            )
            subprocess.run(
                ["git", "checkout", "HEAD", "--"] + paths,
                cwd=self.root,
                check=False,
            )


# --- File Updating Logic with Strict Validation ---
def apply_version_bumps(ctx: ReleaseContext, current_ver: str, new_ver: str) -> None:
    root = ctx.root
    dry_run = ctx.dry_run
    today = datetime.date.today().isoformat()
    c_esc = re.escape(current_ver)

    # 1. Root Cargo.toml
    root_cargo = root / "Cargo.toml"
    info(f"Updating {root_cargo.name} ({current_ver} -> {new_ver})")
    cargo_text = root_cargo.read_text(encoding="utf-8")
    new_cargo, count = re.subn(
        r'(?m)^(\[package\][\s\S]*?^version\s*=\s*)"' + c_esc + r'"',
        rf'\g<1>"{new_ver}"',
        cargo_text,
        count=1,
    )
    if count != 1:
        raise ValueError(f"Failed to match package version in {root_cargo.name}")
    if not dry_run:
        write_file_lf(root_cargo, new_cargo)
    ctx.track(root_cargo)

    # 2. Python bindings Cargo.toml
    py_cargo = root / "bindings" / "python" / "Cargo.toml"
    if py_cargo.exists():
        info(
            f"Updating {py_cargo.relative_to(root).as_posix()} "
            f"({current_ver} -> {new_ver})"
        )
        py_cargo_text = py_cargo.read_text(encoding="utf-8")
        new_py_cargo, count = re.subn(
            r'(?m)^(\[package\][\s\S]*?^version\s*=\s*)"' + c_esc + r'"',
            rf'\g<1>"{new_ver}"',
            py_cargo_text,
            count=1,
        )
        if count != 1:
            raise ValueError(f"Failed to match package version in {py_cargo}")
        if not dry_run:
            write_file_lf(py_cargo, new_py_cargo)
        ctx.track(py_cargo)

    # 3. Python pyproject.toml
    pyproject = root / "bindings" / "python" / "pyproject.toml"
    if pyproject.exists():
        info(
            f"Updating {pyproject.relative_to(root).as_posix()} "
            f"({current_ver} -> {new_ver})"
        )
        pyproject_text = pyproject.read_text(encoding="utf-8")
        new_pyproject, count = re.subn(
            r'(?m)^(\[project\][\s\S]*?^version\s*=\s*)"' + c_esc + r'"',
            rf'\g<1>"{new_ver}"',
            pyproject_text,
            count=1,
        )
        if count != 1:
            raise ValueError(f"Failed to match project version in {pyproject}")
        if not dry_run:
            write_file_lf(pyproject, new_pyproject)
        ctx.track(pyproject)

    # 4. CHANGELOG.md
    changelog = root / "CHANGELOG.md"
    if changelog.exists():
        info(f"Updating {changelog.name} (releasing [Unreleased] as [{new_ver}])")
        cl_text = changelog.read_text(encoding="utf-8")
        if f"## [{new_ver}]" in cl_text:
            raise ValueError(f"CHANGELOG.md already has an entry for ## [{new_ver}]!")
        if "## [Unreleased]" not in cl_text:
            raise ValueError("Could not find '## [Unreleased]' section in CHANGELOG.md")

        new_cl_text = cl_text.replace(
            "## [Unreleased]", f"## [Unreleased]\n\n## [{new_ver}] - {today}", 1
        )
        if not dry_run:
            write_file_lf(changelog, new_cl_text)
        ctx.track(changelog)

    # 5. src/lib.rs (Doc examples & features)
    lib_rs = root / "src" / "lib.rs"
    if lib_rs.exists():
        info(
            f"Updating documentation versions in {lib_rs.relative_to(root).as_posix()}"
        )
        lib_text = lib_rs.read_text(encoding="utf-8")
        lib_text = re.sub(rf'mpatch\s*=\s*"{c_esc}"', f'mpatch = "{new_ver}"', lib_text)
        lib_text = re.sub(
            rf'version\s*=\s*"{c_esc}"', f'version = "{new_ver}"', lib_text
        )
        if not dry_run:
            write_file_lf(lib_rs, lib_text)
        ctx.track(lib_rs)

    # 6. README.md
    readme = root / "README.md"
    if readme.exists():
        info(f"Updating documentation & GPG examples in {readme.name}")
        readme_text = readme.read_text(encoding="utf-8")
        readme_text = re.sub(
            rf'mpatch\s*=\s*"{c_esc}"', f'mpatch = "{new_ver}"', readme_text
        )
        readme_text = re.sub(
            rf'version\s*=\s*"{c_esc}"', f'version = "{new_ver}"', readme_text
        )
        readme_text = re.sub(
            rf"mpatch-x86_64-unknown-linux-gnu-v{c_esc}\.tar\.gz",
            f"mpatch-x86_64-unknown-linux-gnu-v{new_ver}.tar.gz",
            readme_text,
        )
        if not dry_run:
            write_file_lf(readme, readme_text)
        ctx.track(readme)

    # 7. Cargo.lock synchronization via `cargo check`
    cargo_lock = root / "Cargo.lock"
    ctx.track(cargo_lock)
    if not dry_run:
        info("Synchronizing Cargo.lock via 'cargo check --workspace'...")
        run_cmd(["cargo", "check", "--workspace", "--quiet"], cwd=root)


# --- Test & Lint Verification Suites ---
def run_test_and_lint_suite(root: Path, ruff_cmd: list[str] | None) -> None:
    """Runs all linting, formatting, and test suites across Rust and Python."""
    info("Checking Rust code formatting (cargo fmt)...")
    run_cmd(["cargo", "fmt", "--all", "--", "--check"], cwd=root)

    info("Running Clippy linter...")
    run_cmd(
        ["cargo", "clippy", "--all-targets", "--all-features", "--", "-D", "warnings"],
        cwd=root,
    )

    info("Checking Rust documentation and intra-doc links...")
    run_cmd(
        ["cargo", "doc", "--no-deps", "--all-features"],
        cwd=root,
        env={"RUSTDOCFLAGS": "-D warnings"},
    )

    info("Validating Rust crate packaging integrity (cargo package)...")
    run_cmd(["cargo", "package", "--no-deps"], cwd=root)

    info("Running Cargo test suite...")
    run_cmd(["cargo", "test", "--all-features"], cwd=root)

    # Python quality checks (Ruff)
    info("Running Python code quality checks (Ruff)...")
    if not ruff_cmd:
        raise RuntimeError(
            "Required tool 'ruff' is not installed in PATH or the current Python environment.\n"
            "Install it via 'pip install ruff' or use '--skip-tests' to bypass."
        )

    py_dir = root / "bindings" / "python"
    if py_dir.exists():
        info("Checking Python formatting in bindings/python (ruff format --check)...")
        run_cmd(ruff_cmd + ["format", "--check", "."], cwd=py_dir)

        info("Running Ruff linter on bindings/python (ruff check)...")
        run_cmd(ruff_cmd + ["check", "."], cwd=py_dir)

    scripts_dir = root / "scripts"
    if scripts_dir.exists():
        info("Checking Python formatting in scripts/ (ruff format --check)...")
        run_cmd(ruff_cmd + ["format", "--check", "scripts"], cwd=root)

        info("Running Ruff linter on scripts/ (ruff check)...")
        run_cmd(ruff_cmd + ["check", "scripts"], cwd=root)

    # Python test suite (Pytest)
    has_pytest = check_python_module("pytest")
    has_mpatch = check_python_module("mpatch")

    if has_pytest and has_mpatch:
        info("Running Python binding tests (pytest)...")
        run_cmd(
            [sys.executable, "-m", "pytest", "bindings/python/tests"],
            cwd=root,
        )
    elif has_pytest and not has_mpatch:
        warn(
            "pytest is available, but 'mpatch' module is not installed in active Python "
            "environment.\n"
            "Skipping pytest. (To enable: run 'pip install -e bindings/python[test]')"
        )
    else:
        info(
            "Skipping local pytest (pytest not installed in active Python environment)."
        )

    success("All linting, formatting, and test suites passed cleanly.")


# --- Main Orchestration ---
def main() -> None:
    parser = argparse.ArgumentParser(
        description="Sound, production-grade release orchestrator for mpatch.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "level",
        help="Bump type ('patch', 'minor', 'major') or explicit version (e.g. '1.6.5').",
    )
    parser.add_argument(
        "-n",
        "--dry-run",
        action="store_true",
        help="Simulate the release process without modifying files or pushing git tags.",
    )
    parser.add_argument(
        "-y",
        "--yes",
        action="store_true",
        help="Skip all interactive confirmation prompts.",
    )
    parser.add_argument(
        "--skip-tests",
        action="store_true",
        help="Skip running test, clippy, fmt, doc, and ruff validation suites.",
    )
    parser.add_argument(
        "--skip-git-check",
        action="store_true",
        help="Skip branch check (main) and uncommitted changes check.",
    )

    push_group = parser.add_mutually_exclusive_group()
    push_group.add_argument(
        "--push",
        dest="push",
        action="store_true",
        default=None,
        help="Push git commit and tag to remote automatically.",
    )
    push_group.add_argument(
        "--no-push",
        dest="push",
        action="store_false",
        help="Do not push git commit and tag to remote.",
    )

    parser.add_argument(
        "--publish-cargo",
        action="store_true",
        default=False,
        help="Publish the core crate to crates.io after tagging.",
    )

    args = parser.parse_args()
    root = get_repo_root()
    os.chdir(root)

    ctx = ReleaseContext(root, args.dry_run)

    print(f"\n{Style.BOLD}{'=' * 56}{Style.RESET}")
    print(
        f"{Style.BOLD}               mpatch Release Pipeline                {Style.RESET}"
    )
    print(f"{Style.BOLD}{'=' * 56}{Style.RESET}\n")

    if args.dry_run:
        warn("DRY RUN MODE ACTIVE: No files will be modified on disk.\n")

    try:
        # --- Step 1: Pre-flight Tool Diagnostics ---
        issues = run_tool_diagnostics(root, args.skip_tests, args.publish_cargo)
        if issues:
            error("Pre-flight checks identified blocking issues:")
            for issue in issues:
                print(f"  • {issue}", file=sys.stderr)
            abort("Pre-flight environment validation failed.")

        ruff_cmd = find_ruff_cmd()
        current_ver = read_current_version(root)
        new_ver = bump_version(current_ver, args.level)
        tag_name = f"v{new_ver}"

        branch_res = run_cmd(
            ["git", "rev-parse", "--abbrev-ref", "HEAD"], capture_output=True
        )
        current_branch = branch_res.stdout.strip()
        remote_name = get_git_remote(root, current_branch)

        if not args.skip_git_check:
            # Verify git branch is main
            if current_branch != "main":
                warn(f"You are currently on branch '{current_branch}', not 'main'.")
                if not args.yes and not args.dry_run:
                    ans = (
                        input("Do you want to continue anyway? [y/N]: ").strip().lower()
                    )
                    if ans != "y":
                        sys.exit(0)

            # Verify working tree is clean
            status_res = run_cmd(["git", "status", "--porcelain"], capture_output=True)
            if status_res.stdout.strip():
                raise RuntimeError(
                    "Working directory is not clean. Commit or stash changes first."
                )

            # Check if local is behind remote
            fetch_res = subprocess.run(
                ["git", "fetch", remote_name, current_branch, "--quiet"],
                cwd=root,
                capture_output=True,
                text=True,
            )
            if fetch_res.returncode == 0:
                behind_check = subprocess.run(
                    [
                        "git",
                        "rev-list",
                        f"HEAD..{remote_name}/{current_branch}",
                        "--count",
                    ],
                    cwd=root,
                    capture_output=True,
                    text=True,
                )
                if (
                    behind_check.returncode == 0
                    and int(behind_check.stdout.strip() or 0) > 0
                ):
                    behind_count = behind_check.stdout.strip()
                    raise RuntimeError(
                        f"Local branch '{current_branch}' is behind "
                        f"{remote_name}/{current_branch} by {behind_count} commit(s). "
                        "Run 'git pull' first."
                    )
            else:
                warn(
                    f"Could not fetch from remote '{remote_name}'. "
                    "Skipping remote sync check."
                )

            # Check for existing tag collision locally or on remote
            tag_check = subprocess.run(
                ["git", "rev-parse", "-q", "--verify", f"refs/tags/{tag_name}"],
                cwd=root,
                capture_output=True,
            )
            if tag_check.returncode == 0:
                raise RuntimeError(f"Git tag '{tag_name}' already exists locally!")

            remote_tag_check = subprocess.run(
                ["git", "ls-remote", "--tags", remote_name, tag_name],
                cwd=root,
                capture_output=True,
                text=True,
            )
            if remote_tag_check.returncode == 0 and remote_tag_check.stdout.strip():
                raise RuntimeError(
                    f"Git tag '{tag_name}' already exists on remote {remote_name}!"
                )

        success(f"Current version: {current_ver}")
        success(f"Target version:  {new_ver}")

        # Check that CHANGELOG [Unreleased] is not empty
        changelog_path = root / "CHANGELOG.md"
        if changelog_path.exists():
            changelog_content = changelog_path.read_text(encoding="utf-8")
            unreleased_match = re.search(
                r"## \[Unreleased\]\s+([\s\S]*?)(?=(?:\r?\n)## \[|$)",
                changelog_content,
            )
            if not unreleased_match or not unreleased_match.group(1).strip():
                warn("CHANGELOG.md '## [Unreleased]' section appears empty!")
                if not args.yes and not args.dry_run:
                    ans = (
                        input("Continue release without changelog entries? [y/N]: ")
                        .strip()
                        .lower()
                    )
                    if ans != "y":
                        sys.exit(0)

        # --- Step 2: Test & Lint Suite Verification ---
        if not args.skip_tests:
            print("")
            info("Step 2: Running Comprehensive Test & Lint Suite")
            run_test_and_lint_suite(root, ruff_cmd)
        else:
            warn("Step 2: Test and lint suite SKIPPED (--skip-tests).")

        # --- Step 3: Confirmation Prompt ---
        print("")
        info("Step 3: Ready to Apply Version Bump")
        print(f"  • Current Version: {Style.BOLD}{current_ver}{Style.RESET}")
        print(f"  • Target Version:  {Style.BOLD}{new_ver}{Style.RESET}")
        print(f"  • Git Tag:         {Style.BOLD}{tag_name}{Style.RESET}")
        print(f"  • Remote:          {Style.BOLD}{remote_name}{Style.RESET}")

        if not args.yes and not args.dry_run:
            confirm = (
                input("\nProceed with version bump and git tag? [y/N]: ")
                .strip()
                .lower()
            )
            if confirm != "y":
                info("Release aborted by user.")
                sys.exit(0)

        # --- Step 4: Apply Version Bumps ---
        print("")
        info("Step 4: Updating Version References in Files")
        apply_version_bumps(ctx, current_ver, new_ver)
        success(f"Updated {len(ctx.modified_files)} files.")

        # Post-bump syntax verification before committing
        if not args.dry_run and not args.skip_tests:
            info("Verifying formatting of bumped files before commit...")
            run_cmd(["cargo", "fmt", "--all", "--", "--check"], cwd=root)

        # --- Step 5: Git Commit & Tag ---
        print("")
        info("Step 5: Git Commit and Tagging")
        commit_msg = f"chore: release {tag_name}"

        if args.dry_run:
            info(
                f"[DRY RUN] Would execute: git add "
                f"{' '.join(f.name for f in ctx.modified_files)}"
            )
            info(f"[DRY RUN] Would execute: git commit -m '{commit_msg}'")
            info(
                f"[DRY RUN] Would execute: git tag -a {tag_name} -m 'Release {tag_name}'"
            )
        else:
            run_cmd(
                ["git", "add"]
                + [f.relative_to(root).as_posix() for f in ctx.modified_files],
                cwd=root,
            )
            run_cmd(["git", "commit", "-m", commit_msg], cwd=root)
            ctx.committed = True
            run_cmd(
                ["git", "tag", "-a", tag_name, "-m", f"Release {tag_name}"],
                cwd=root,
            )
            success(f"Created git commit '{commit_msg}' and tag '{tag_name}'.")

        # --- Step 6: Git Push ---
        print("")
        info("Step 6: Pushing to Remote Repository")
        should_push = args.push
        if should_push is None and not args.dry_run:
            if args.yes:
                should_push = True
            else:
                ans = (
                    input(f"Push commit and tag '{tag_name}' to {remote_name}? [Y/n]: ")
                    .strip()
                    .lower()
                )
                should_push = ans != "n"

        if should_push:
            if args.dry_run:
                info(
                    f"[DRY RUN] Would execute: git push {remote_name} HEAD "
                    f"and git push {remote_name} {tag_name}"
                )
            else:
                run_cmd(["git", "push", remote_name, "HEAD"], cwd=root)
                run_cmd(["git", "push", remote_name, tag_name], cwd=root)
                success(f"Pushed commit and tag '{tag_name}' to {remote_name}.")
        else:
            warn(
                f"Skipped push. Run manually: git push {remote_name} HEAD "
                f"&& git push {remote_name} {tag_name}"
            )

        # --- Step 7: Crates.io Publishing ---
        print("")
        info("Step 7: Publishing to Crates.io")
        should_publish_cargo = args.publish_cargo
        if not should_publish_cargo and not args.dry_run and not args.yes:
            ans = (
                input("Do you want to publish the Rust crate to Crates.io now? [y/N]: ")
                .strip()
                .lower()
            )
            should_publish_cargo = ans == "y"

        if should_publish_cargo:
            if args.dry_run:
                info("[DRY RUN] Would execute: cargo publish -p mpatch")
            else:
                try:
                    run_cmd(["cargo", "publish", "-p", "mpatch"], cwd=root)
                    success("Published mpatch to Crates.io!")
                except Exception as e:
                    warn(
                        f"Crates.io publish failed ({e}). "
                        "You can publish manually later with: cargo publish -p mpatch"
                    )
        else:
            info(
                "Skipped cargo publish. Publish manually later using: "
                "cargo publish -p mpatch"
            )

        # --- Step 8: Summary & Guidance ---
        print(f"\n{Style.BOLD}{'=' * 56}{Style.RESET}")
        print(
            f"{Style.GREEN}{Style.BOLD}             "
            f"Release {tag_name} Initialized!            {Style.RESET}"
        )
        print(f"{Style.BOLD}{'=' * 56}{Style.RESET}\n")
        print("Next steps in the release lifecycle:")
        print(f" 1. {Style.BOLD}GitHub Actions Compilation:{Style.RESET}")
        print(
            f"    Pushing tag '{tag_name}' triggered `.github/workflows/release.yml`."
        )
        print(
            f"    It compiles multi-platform binaries and creates a "
            f"{Style.BOLD}Draft Release{Style.RESET}."
        )
        print(f" 2. {Style.BOLD}Publish Draft on GitHub & Trigger PyPI:{Style.RESET}")
        print("    • Navigate to your repository -> Releases.")
        print(
            f"    • Open Draft Release {tag_name} and click "
            f"{Style.GREEN}'Publish Release'{Style.RESET}."
        )
        print("    • This automatically triggers `.github/workflows/pypi.yml` to build")
        print("      and publish all Python wheels to PyPI.")
        print("")

    except KeyboardInterrupt:
        ctx.rollback()
        abort("Release interrupted by user.")
    except Exception as exc:
        ctx.rollback()
        abort(str(exc))


if __name__ == "__main__":
    main()
