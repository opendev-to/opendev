"""PresentPlan tool - Present a plan file for user approval."""

from __future__ import annotations

from pathlib import Path
from typing import Any, TYPE_CHECKING

if TYPE_CHECKING:
    from swecli.ui_textual.ui_callback import UICallback


class PresentPlanTool:
    """Tool for presenting a completed plan to the user for approval.

    After the Planner subagent writes a plan to a file, the main agent
    calls this tool to present the plan and get user sign-off before
    implementation. No mode_manager dependency — takes the plan file
    path as a parameter.
    """

    @property
    def name(self) -> str:
        return "present_plan"

    def execute(
        self,
        plan_file_path: str = "",
        ui_callback: "UICallback | None" = None,
        session_manager: Any = None,
        **kwargs: Any,
    ) -> dict[str, Any]:
        """Read plan from file and present for approval.

        Args:
            plan_file_path: Absolute path to the plan file.
            ui_callback: UI callback for user approval dialog.
            session_manager: Session manager for metadata storage.
            **kwargs: Additional context (ignored).

        Returns:
            Result dict with approval decision.
        """
        if not plan_file_path:
            return {
                "success": False,
                "error": "plan_file_path is required",
                "output": None,
            }

        plan_path = Path(plan_file_path).expanduser()

        if not plan_path.exists():
            return {
                "success": False,
                "error": f"Plan file not found: {plan_file_path}",
                "output": "Plan file does not exist. Spawn a Planner subagent "
                "to create the plan first.",
            }

        plan_content = plan_path.read_text(encoding="utf-8")

        if not plan_content or not plan_content.strip():
            return {
                "success": False,
                "error": f"Plan file is empty: {plan_file_path}",
                "output": "Plan file exists but is empty. Spawn a Planner "
                "subagent to write the plan first.",
            }

        # Store plan_file_path in session metadata
        if session_manager:
            try:
                session = session_manager.get_current_session()
                if session:
                    session.metadata["plan_file_path"] = plan_file_path
                    session_manager.save_current_session()
            except Exception:
                pass  # Non-fatal

        # Index the plan
        try:
            from swecli.core.paths import get_paths
            from swecli.core.runtime.plan_index import PlanIndex

            plans_dir = get_paths().global_plans_dir
            plans_dir.mkdir(parents=True, exist_ok=True)
            plan_name = plan_path.stem

            session_id = None
            if session_manager:
                session = session_manager.get_current_session()
                if session:
                    session_id = session.id

            project_path = str(get_paths().working_dir)
            index = PlanIndex(plans_dir)
            index.add_entry(plan_name, session_id=session_id, project_path=project_path)
        except Exception:
            pass  # Index write failure is non-fatal

        # Display plan content as a separate box before the approval panel
        if ui_callback and hasattr(ui_callback, "display_plan_content"):
            ui_callback.display_plan_content(plan_content)

        # Present approval dialog via UI callback
        if ui_callback and hasattr(ui_callback, "request_plan_approval"):
            result = ui_callback.request_plan_approval(plan_content=plan_content)

            action = result.get("action")  # "approve_auto", "approve", "modify", "reject"

            if action == "approve_auto":
                return {
                    "success": True,
                    "output": "Plan approved! Proceed with implementation. "
                    "Work through each step in order.",
                    "plan_approved": True,
                    "auto_approve": True,
                    "plan_content": plan_content,
                }

            elif action == "approve":
                return {
                    "success": True,
                    "output": "Plan approved! Proceed with implementation. "
                    "Work through each step in order.",
                    "plan_approved": True,
                    "auto_approve": False,
                    "plan_content": plan_content,
                }

            elif action == "modify":
                feedback = result.get("feedback", "")
                return {
                    "success": True,
                    "output": f"User requested modifications to the plan.\n\n"
                    f"{feedback}\n\n"
                    "Re-spawn the Planner subagent with this feedback and the "
                    f"same plan file path ({plan_file_path}), then call "
                    "present_plan again.",
                    "plan_approved": False,
                    "requires_modification": True,
                }

            else:  # reject / unknown
                return {
                    "success": True,
                    "output": "Plan rejected. User does not want to proceed. "
                    "Ask the user how they'd like to proceed.",
                    "plan_approved": False,
                    "plan_rejected": True,
                }

        # Fallback if no UI callback: auto-approve (non-interactive contexts)
        return {
            "success": True,
            "output": "Plan completed. Proceed with implementation.",
            "plan_approved": True,
            "plan_content": plan_content,
        }
