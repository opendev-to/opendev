"""Base classes for modal dialogs."""

from abc import ABC, abstractmethod
from typing import Optional

from prompt_toolkit.application import Application
from prompt_toolkit.layout.layout import Layout
from prompt_toolkit.styles import Style


class BaseModal(ABC):
    """Base class for modal dialogs."""

    def __init__(self):
        """Initialize base modal."""
        self.result: Optional[str] = None

    @abstractmethod
    def create_layout(self) -> Layout:
        """Create the modal layout.

        Returns:
            Configured Layout instance
        """
        pass

    @abstractmethod
    def create_style(self) -> Style:
        """Create the modal style.

        Returns:
            Configured Style instance
        """
        pass

    @abstractmethod
    def create_key_bindings(self):
        """Create key bindings for the modal.

        Returns:
            KeyBindings instance
        """
        pass

    @abstractmethod
    def get_default_result(self) -> str:
        """Get default result if modal is closed without explicit action.

        Returns:
            Default result string
        """
        pass

    def show(self) -> str:
        """Show the modal dialog.

        Returns:
            Modal result
        """
        import asyncio

        self.result = None

        layout = self.create_layout()
        key_bindings = self.create_key_bindings()
        style = self.create_style()

        app = Application(
            layout=layout,
            key_bindings=key_bindings,
            style=style,
            full_screen=False,
            mouse_support=False,
        )

        # Check if we're already in a running event loop
        try:
            loop = asyncio.get_running_loop()
            # We're in a running event loop - apply nest_asyncio to allow nested event loops
            try:
                import nest_asyncio
                nest_asyncio.apply(loop)
            except ImportError:
                # nest_asyncio not available, try anyway
                pass
            app.run()
        except RuntimeError:
            # No running event loop, run normally
            app.run()

        return self.result or self.get_default_result()


class NavigationMixin:
    """Mixin for modal navigation functionality."""

    def __init__(self, max_items: int):
        """Initialize navigation mixin.

        Args:
            max_items: Maximum number of items
        """
        self.current_index = 0
        self.max_items = max_items

    def can_go_next(self) -> bool:
        """Check if can go to next item.

        Returns:
            True if next item exists
        """
        return self.current_index < self.max_items - 1

    def can_go_previous(self) -> bool:
        """Check if can go to previous item.

        Returns:
            True if previous item exists
        """
        return self.current_index > 0

    def go_next(self) -> bool:
        """Go to next item.

        Returns:
            True if moved to next item
        """
        if self.can_go_next():
            self.current_index += 1
            return True
        return False

    def go_previous(self) -> bool:
        """Go to previous item.

        Returns:
            True if moved to previous item
        """
        if self.can_go_previous():
            self.current_index -= 1
            return True
        return False

    def set_max_items(self, max_items: int) -> None:
        """Update maximum items count.

        Args:
            max_items: New maximum items count
        """
        self.max_items = max_items
        # Adjust current index if needed
        if self.current_index >= self.max_items:
            self.current_index = max(0, self.max_items - 1)