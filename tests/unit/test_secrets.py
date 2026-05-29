"""Unit tests for the secrets manager (env/file providers, constant-time comparison)."""

from __future__ import annotations

import os
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "bugswarm-gateway" / "src"))

from gateway.types import _load_api_key, secrets_compare


def test_env_provider_returns_env_var():
    os.environ["TEST_SECRET_KEY"] = "sk-test-secret-12345"
    try:
        result = _load_api_key("TEST_SECRET_KEY")
        assert result == "sk-test-secret-12345"
    finally:
        del os.environ["TEST_SECRET_KEY"]


def test_env_provider_missing_returns_empty():
    if "MISSING_SECRET_KEY" in os.environ:
        del os.environ["MISSING_SECRET_KEY"]
    result = _load_api_key("MISSING_SECRET_KEY")
    assert result == ""


def test_file_provider_reads_secret():
    with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".secret") as f:
        f.write("sk-file-secret\n")
        secret_path = f.name

    os.environ["BGSWARM_SECRETS_PROVIDER"] = "file"
    os.environ["TEST_FILE_KEY_FILE"] = secret_path
    try:
        result = _load_api_key("TEST_FILE_KEY")
        assert result == "sk-file-secret"
    finally:
        os.environ.pop("BGSWARM_SECRETS_PROVIDER", None)
        os.environ.pop("TEST_FILE_KEY_FILE", None)
        os.unlink(secret_path)


def test_file_provider_missing_file_returns_empty():
    os.environ["BGSWARM_SECRETS_PROVIDER"] = "file"
    os.environ["MISSING_FILE_KEY_FILE"] = "/tmp/nonexistent_secret_file_12345"
    try:
        result = _load_api_key("MISSING_FILE_KEY")
        assert result == ""
    finally:
        os.environ.pop("BGSWARM_SECRETS_PROVIDER", None)
        os.environ.pop("MISSING_FILE_KEY_FILE", None)


def test_file_provider_fallback_path():
    os.environ["BGSWARM_SECRETS_PROVIDER"] = "file"
    try:
        result = _load_api_key("NONEXISTENT_KEY")
        assert result == ""
    finally:
        os.environ.pop("BGSWARM_SECRETS_PROVIDER", None)


def test_secrets_compare_equal():
    assert secrets_compare("sk-test-secret", "sk-test-secret") is True


def test_secrets_compare_different():
    assert secrets_compare("sk-test-secret", "sk-other-secret") is False


def test_secrets_compare_empty():
    assert secrets_compare("", "") is True
    assert secrets_compare("sk-test", "") is False


def test_secrets_compare_unicode():
    assert secrets_compare("héllo-wörld", "héllo-wörld") is True
    assert secrets_compare("héllo", "hello") is False
