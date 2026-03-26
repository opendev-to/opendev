//! UI widgets for the TUI application.

pub mod background_tasks;
pub mod conversation;
pub mod input;
pub mod nested_tool;
pub mod progress;
pub mod spinner;
pub mod status_bar;
pub mod thinking_verbs;
pub mod toast;
pub mod todo_panel;
pub mod tool_display;
pub mod welcome_panel;

pub use background_tasks::TaskWatcherPanel;
pub use conversation::ConversationWidget;
pub use input::InputWidget;
pub use nested_tool::{NestedToolWidget, SubagentDisplayState};
pub use progress::TaskProgress;
pub use spinner::SpinnerState;
pub use status_bar::StatusBarWidget;
pub use toast::{Toast, ToastLevel, ToastWidget};
pub use todo_panel::{TodoDisplayItem, TodoDisplayStatus, TodoPanelWidget};
pub use welcome_panel::{WelcomePanelState, WelcomePanelWidget};
