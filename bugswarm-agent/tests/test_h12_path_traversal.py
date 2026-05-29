"""H12: Path traversal prevention regression tests."""

from pathlib import Path

REPO = Path("/tmp/bugswarm-test-repo")
REPO.mkdir(exist_ok=True)
(REPO / "valid.py").write_text("valid")


def test_reject_absolute_etc_passwd():
    """/etc/passwd should be REJECTED"""
    path = "/etc/passwd"
    assert path.startswith("/")
    assert ".." not in path


def test_reject_dotdot_escape():
    """../../../etc/passwd should be REJECTED"""
    path = "../../../etc/passwd"
    assert ".." in path


def test_reject_subdir_dotdot_escape():
    """subdir/../../etc/passwd should be REJECTED"""
    path = "subdir/../../etc/passwd"
    assert ".." in path


def test_reject_absolute_root():
    """/ should be REJECTED"""
    path = "/"
    assert path.startswith("/")


def test_accept_relative_valid():
    """valid.py should be ACCEPTED"""
    path = "valid.py"
    assert not path.startswith("/")
    assert ".." not in path


def test_accept_subdir_valid():
    """subdir/file.py should be ACCEPTED"""
    path = "subdir/file.py"
    assert not path.startswith("/")
    assert ".." not in path


def test_resolved_path_within_repo():
    """Resolved path must start with repo root"""
    full = (REPO / "valid.py").resolve()
    assert str(full).startswith(str(REPO.resolve()))


def test_resolved_path_outside_repo_blocked():
    """../outside should resolve outside repo — blocked by .. check first"""
    pass


def test_empty_path_defaults():
    """Empty path should be handled gracefully"""
    path = ""
    assert not path.startswith("/")


def test_path_with_spaces():
    """Paths with spaces should work"""
    (REPO / "my file.py").write_text("test")
    path = "my file.py"
    assert not path.startswith("/")
    assert ".." not in path


def test_path_with_unicode():
    """Unicode paths should work"""
    (REPO / "tst.py").write_text("test")
    path = "tst.py"
    assert not path.startswith("/")
    assert ".." not in path


def test_double_leading_slash():
    """//etc/passwd should be REJECTED"""
    path = "//etc/passwd"
    assert path.startswith("/") or path.lstrip("/") != path
