"""Subagent specifications."""

from .ask_user import ASK_USER_SUBAGENT
from .code_explorer import CODE_EXPLORER_SUBAGENT
from .planner import PLANNER_SUBAGENT
from .pr_reviewer import PR_REVIEWER_SUBAGENT
from .project_init import PROJECT_INIT_SUBAGENT
from .security_reviewer import SECURITY_REVIEWER_SUBAGENT
from .web_clone import WEB_CLONE_SUBAGENT
from .web_generator import WEB_GENERATOR_SUBAGENT

ALL_SUBAGENTS = [
    ASK_USER_SUBAGENT,
    CODE_EXPLORER_SUBAGENT,
    PLANNER_SUBAGENT,
    PR_REVIEWER_SUBAGENT,
    PROJECT_INIT_SUBAGENT,
    SECURITY_REVIEWER_SUBAGENT,
    WEB_CLONE_SUBAGENT,
    WEB_GENERATOR_SUBAGENT,
]
