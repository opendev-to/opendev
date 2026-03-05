"""Tests for project-scoped session path encoding."""

from pathlib import Path

from swecli.core.paths import (
    Paths,
    encode_project_path,
    PROJECTS_DIR_NAME,
    FALLBACK_PROJECT_DIR_NAME,
)


class TestEncodeProjectPath:
    def test_basic_encoding(self):
        assert encode_project_path(Path("/Users/foo/bar")) == "-Users-foo-bar"

    def test_root_path(self):
        assert encode_project_path(Path("/")) == "-"

    def test_trailing_slash_ignored(self):
        # Path.resolve() normalises trailing slashes
        result = encode_project_path(Path("/Users/foo/bar"))
        assert result == "-Users-foo-bar"

    def test_relative_path_resolved(self, tmp_path, monkeypatch):
        monkeypatch.chdir(tmp_path)
        encoded = encode_project_path(Path("."))
        assert encoded == str(tmp_path.resolve()).replace("/", "-")


class TestProjectSessionsDir:
    def test_returns_encoded_subdir(self, tmp_path):
        paths = Paths()
        result = paths.project_sessions_dir(Path("/Users/foo/bar"))
        assert result == paths.global_projects_dir / "-Users-foo-bar"

    def test_different_dirs_produce_different_paths(self):
        paths = Paths()
        a = paths.project_sessions_dir(Path("/a"))
        b = paths.project_sessions_dir(Path("/b"))
        assert a != b


class TestGlobalProjectsDir:
    def test_projects_dir_constant(self):
        assert PROJECTS_DIR_NAME == "projects"

    def test_fallback_constant(self):
        assert FALLBACK_PROJECT_DIR_NAME == "-unknown-"

    def test_global_projects_dir_under_global(self):
        paths = Paths()
        assert paths.global_projects_dir == paths.global_dir / PROJECTS_DIR_NAME
