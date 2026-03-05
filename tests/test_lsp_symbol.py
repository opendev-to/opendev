"""Tests for LSP symbol classes and name path matching."""

import pytest
from swecli.core.context_engineering.tools.lsp.symbol import Symbol, SymbolKind, NamePathMatcher, find_symbols_by_pattern


class TestSymbolKind:
    """Tests for SymbolKind enum."""

    def test_from_value_valid(self):
        """Test creating SymbolKind from valid integer."""
        assert SymbolKind.from_value(5) == SymbolKind.CLASS
        assert SymbolKind.from_value(12) == SymbolKind.FUNCTION
        assert SymbolKind.from_value(6) == SymbolKind.METHOD

    def test_from_value_invalid(self):
        """Test creating SymbolKind from invalid integer returns VARIABLE."""
        assert SymbolKind.from_value(999) == SymbolKind.VARIABLE

    def test_is_container(self):
        """Test is_container method."""
        assert SymbolKind.CLASS.is_container() is True
        assert SymbolKind.MODULE.is_container() is True
        assert SymbolKind.FUNCTION.is_container() is False
        assert SymbolKind.VARIABLE.is_container() is False


class TestSymbol:
    """Tests for Symbol class."""

    def test_from_document_symbol(self):
        """Test creating Symbol from LSP DocumentSymbol."""
        data = {
            "name": "MyClass",
            "kind": 5,  # CLASS
            "range": {
                "start": {"line": 10, "character": 0},
                "end": {"line": 20, "character": 1},
            },
            "children": [
                {
                    "name": "my_method",
                    "kind": 6,  # METHOD
                    "range": {
                        "start": {"line": 12, "character": 4},
                        "end": {"line": 15, "character": 8},
                    },
                }
            ],
        }

        symbol = Symbol.from_document_symbol(data, "/path/to/file.py")

        assert symbol.name == "MyClass"
        assert symbol.kind == SymbolKind.CLASS
        assert symbol.file_path == "/path/to/file.py"
        assert symbol.start_line == 10
        assert symbol.end_line == 20
        assert len(symbol.children) == 1
        assert symbol.children[0].name == "my_method"
        assert symbol.children[0].parent == symbol

    def test_from_symbol_information(self):
        """Test creating Symbol from LSP SymbolInformation."""
        data = {
            "name": "my_function",
            "kind": 12,  # FUNCTION
            "location": {
                "uri": "file:///path/to/file.py",
                "range": {
                    "start": {"line": 5, "character": 0},
                    "end": {"line": 10, "character": 0},
                },
            },
            "containerName": "module",
        }

        symbol = Symbol.from_symbol_information(data)

        assert symbol.name == "my_function"
        assert symbol.kind == SymbolKind.FUNCTION
        assert symbol.file_path == "/path/to/file.py"
        assert symbol.container_name == "module"

    def test_name_path(self):
        """Test name_path property with nested symbols."""
        parent = Symbol(
            name="MyClass",
            kind=SymbolKind.CLASS,
            file_path="/test.py",
            start_line=0,
            start_character=0,
            end_line=10,
            end_character=0,
        )

        child = Symbol(
            name="my_method",
            kind=SymbolKind.METHOD,
            file_path="/test.py",
            start_line=2,
            start_character=4,
            end_line=5,
            end_character=0,
            parent=parent,
        )

        assert parent.name_path == "MyClass"
        assert child.name_path == "MyClass.my_method"

    def test_get_body(self):
        """Test extracting symbol body from file content."""
        file_content = """def my_function():
    return 42

class MyClass:
    pass
"""
        symbol = Symbol(
            name="my_function",
            kind=SymbolKind.FUNCTION,
            file_path="/test.py",
            start_line=0,
            start_character=0,
            end_line=1,
            end_character=13,
        )

        body = symbol.get_body(file_content)
        assert "def my_function():" in body
        assert "return 42" in body

    def test_contains_position(self):
        """Test position containment check."""
        symbol = Symbol(
            name="test",
            kind=SymbolKind.FUNCTION,
            file_path="/test.py",
            start_line=5,
            start_character=0,
            end_line=10,
            end_character=10,
        )

        # Inside
        assert symbol.contains_position(7, 5) is True
        # On start line
        assert symbol.contains_position(5, 5) is True
        # Before start
        assert symbol.contains_position(4, 0) is False
        # After end
        assert symbol.contains_position(11, 0) is False

    def test_iter_descendants(self):
        """Test iterating over all descendants."""
        parent = Symbol(
            name="Parent",
            kind=SymbolKind.CLASS,
            file_path="/test.py",
            start_line=0,
            start_character=0,
            end_line=20,
            end_character=0,
        )

        child1 = Symbol(
            name="child1",
            kind=SymbolKind.METHOD,
            file_path="/test.py",
            start_line=2,
            start_character=0,
            end_line=5,
            end_character=0,
            parent=parent,
        )

        grandchild = Symbol(
            name="grandchild",
            kind=SymbolKind.VARIABLE,
            file_path="/test.py",
            start_line=3,
            start_character=0,
            end_line=3,
            end_character=10,
            parent=child1,
        )

        child1.children.append(grandchild)
        parent.children.append(child1)

        descendants = parent.iter_descendants()
        assert len(descendants) == 2
        assert child1 in descendants
        assert grandchild in descendants


class TestNamePathMatcher:
    """Tests for NamePathMatcher class."""

    def test_exact_match(self):
        """Test exact name path matching."""
        matcher = NamePathMatcher("MyClass.method")

        assert matcher.matches_name_path("MyClass.method") is True
        assert matcher.matches_name_path("OtherClass.method") is False

    def test_partial_match(self):
        """Test partial path matching (suffix)."""
        matcher = NamePathMatcher("MyClass.method")

        # Pattern should match as suffix
        assert matcher.matches_name_path("module.MyClass.method") is True
        assert matcher.matches_name_path("pkg.module.MyClass.method") is True

    def test_name_only_match(self):
        """Test single name matching."""
        matcher = NamePathMatcher("method")

        assert matcher.matches_name_path("method") is True
        assert matcher.matches_name_path("MyClass.method") is True
        assert matcher.matches_name_path("pkg.MyClass.method") is True
        assert matcher.matches_name_path("other_method") is False

    def test_wildcard_match(self):
        """Test wildcard pattern matching."""
        matcher = NamePathMatcher("My*")

        assert matcher.matches_name_path("MyClass") is True
        assert matcher.matches_name_path("MyFunction") is True
        assert matcher.matches_name_path("YourClass") is False

    def test_wildcard_suffix_match(self):
        """Test wildcard with suffix matching."""
        matcher = NamePathMatcher("MyClass.*")

        assert matcher.matches_name_path("MyClass.method1") is True
        assert matcher.matches_name_path("MyClass.method2") is True
        assert matcher.matches_name_path("OtherClass.method") is False

    def test_matches_symbol(self):
        """Test matching against Symbol object."""
        parent = Symbol(
            name="MyClass",
            kind=SymbolKind.CLASS,
            file_path="/test.py",
            start_line=0,
            start_character=0,
            end_line=10,
            end_character=0,
        )

        child = Symbol(
            name="method",
            kind=SymbolKind.METHOD,
            file_path="/test.py",
            start_line=2,
            start_character=0,
            end_line=5,
            end_character=0,
            parent=parent,
        )

        matcher = NamePathMatcher("MyClass.method")
        assert matcher.matches(child) is True

        matcher2 = NamePathMatcher("method")
        assert matcher2.matches(child) is True


class TestFindSymbolsByPattern:
    """Tests for find_symbols_by_pattern function."""

    def test_find_by_name(self):
        """Test finding symbols by name."""
        symbols = [
            Symbol(
                name="func1",
                kind=SymbolKind.FUNCTION,
                file_path="/test.py",
                start_line=0,
                start_character=0,
                end_line=5,
                end_character=0,
            ),
            Symbol(
                name="func2",
                kind=SymbolKind.FUNCTION,
                file_path="/test.py",
                start_line=10,
                start_character=0,
                end_line=15,
                end_character=0,
            ),
        ]

        results = find_symbols_by_pattern(symbols, "func1")
        assert len(results) == 1
        assert results[0].name == "func1"

    def test_find_with_wildcard(self):
        """Test finding symbols with wildcard pattern."""
        symbols = [
            Symbol(
                name="test_func1",
                kind=SymbolKind.FUNCTION,
                file_path="/test.py",
                start_line=0,
                start_character=0,
                end_line=5,
                end_character=0,
            ),
            Symbol(
                name="test_func2",
                kind=SymbolKind.FUNCTION,
                file_path="/test.py",
                start_line=10,
                start_character=0,
                end_line=15,
                end_character=0,
            ),
            Symbol(
                name="other_func",
                kind=SymbolKind.FUNCTION,
                file_path="/test.py",
                start_line=20,
                start_character=0,
                end_line=25,
                end_character=0,
            ),
        ]

        results = find_symbols_by_pattern(symbols, "test_*")
        assert len(results) == 2

    def test_find_in_descendants(self):
        """Test finding symbols in nested structure."""
        parent = Symbol(
            name="MyClass",
            kind=SymbolKind.CLASS,
            file_path="/test.py",
            start_line=0,
            start_character=0,
            end_line=20,
            end_character=0,
        )

        child = Symbol(
            name="method",
            kind=SymbolKind.METHOD,
            file_path="/test.py",
            start_line=2,
            start_character=0,
            end_line=5,
            end_character=0,
            parent=parent,
        )

        parent.children.append(child)

        results = find_symbols_by_pattern([parent], "method")
        assert len(results) == 1
        assert results[0].name == "method"
