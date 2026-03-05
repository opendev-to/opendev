"""LLM calling with progress display."""

import random
import time


class LLMCaller:
    """Handles LLM invocation with progress display."""

    # Fancy verbs for the thinking spinner - randomly selected for variety (100 verbs!)
    THINKING_VERBS = [
        "Thinking",
        "Processing",
        "Analyzing",
        "Computing",
        "Synthesizing",
        "Orchestrating",
        "Crafting",
        "Brewing",
        "Composing",
        "Contemplating",
        "Formulating",
        "Strategizing",
        "Architecting",
        "Designing",
        "Manifesting",
        "Conjuring",
        "Weaving",
        "Pondering",
        "Calculating",
        "Deliberating",
        "Ruminating",
        "Meditating",
        "Scheming",
        "Envisioning",
        "Imagining",
        "Conceptualizing",
        "Ideating",
        "Brainstorming",
        "Innovating",
        "Engineering",
        "Assembling",
        "Constructing",
        "Building",
        "Forging",
        "Molding",
        "Sculpting",
        "Fashioning",
        "Shaping",
        "Rendering",
        "Materializing",
        "Realizing",
        "Actualizing",
        "Executing",
        "Implementing",
        "Deploying",
        "Launching",
        "Initiating",
        "Activating",
        "Energizing",
        "Catalyzing",
        "Accelerating",
        "Optimizing",
        "Refining",
        "Polishing",
        "Perfecting",
        "Enhancing",
        "Augmenting",
        "Amplifying",
        "Boosting",
        "Elevating",
        "Transcending",
        "Transforming",
        "Evolving",
        "Adapting",
        "Modifying",
        "Adjusting",
        "Tuning",
        "Calibrating",
        "Aligning",
        "Harmonizing",
        "Synchronizing",
        "Integrating",
        "Consolidating",
        "Unifying",
        "Merging",
        "Blending",
        "Fusing",
        "Combining",
        "Mixing",
        "Synthesizing",
        "Aggregating",
        "Compiling",
        "Gathering",
        "Collecting",
        "Assembling",
        "Organizing",
        "Structuring",
        "Arranging",
        "Ordering",
        "Sorting",
        "Classifying",
        "Categorizing",
        "Grouping",
        "Clustering",
        "Partitioning",
        "Dividing",
        "Separating",
        "Filtering",
        "Selecting",
        "Choosing",
        "Picking",
        "Deciding",
        "Determining",
        "Resolving",
        "Solving",
        "Addressing",
        "Tackling",
        "Handling",
        "Managing",
        "Directing",
        "Guiding",
        "Leading",
        "Navigating",
        "Steering",
        "Piloting",
        "Charting",
        "Mapping",
        "Planning",
        "Plotting",
        "Devising",
        "Preparing",
        "Readying",
        "Priming",
        "Setting",
        "Configuring",
        "Establishing",
        "Initializing",
        "Launching",
        "Starting",
        "Beginning",
        "Commencing",
        "Kickstarting",
        "Jumpstarting",
        "Sparking",
        "Igniting",
        "Triggering",
        "Activating",
        "Engaging",
        "Mobilizing",
        "Rallying",
        "Assembling",
        "Marshaling",
        "Coordinating",
        "Synchronizing",
        "Aligning",
        "Balancing",
        "Calibrating",
        "Tuning",
        "Adjusting",
    ]

    def __init__(self, console):
        """Initialize LLM caller.

        Args:
            console: Rich console for output
        """
        self.console = console
        self._current_task_monitor = None

    def call_llm_with_progress(
        self, agent, messages, task_monitor, thinking_visible: bool = True
    ) -> tuple:
        """Call LLM with progress display.

        Args:
            agent: Agent to use
            messages: Message history
            task_monitor: Task monitor for tracking
            thinking_visible: If False, exclude think tool from schemas

        Returns:
            Tuple of (response, latency_ms)
        """
        import logging
        import traceback
        logger = logging.getLogger(__name__)

        from swecli.ui_textual.components.task_progress import TaskProgressDisplay

        # Get random thinking verb
        thinking_verb = random.choice(self.THINKING_VERBS)
        task_monitor.start(thinking_verb, initial_tokens=0)

        # Track current monitor for interrupt support
        self._current_task_monitor = task_monitor
        from swecli.ui_textual.debug_logger import debug_log
        debug_log("LLMCaller", f"SET _current_task_monitor={task_monitor}")

        # Create progress display with live updates
        progress = TaskProgressDisplay(self.console, task_monitor)
        progress.start()

        # Removed 50ms delay - UI renders fast enough without it for instant ESC interrupt response

        try:
            # DEBUG: Log before LLM call
            logger.debug(f"[LLM_CALLER] Calling agent.call_llm with {len(messages)} messages")
            logger.debug(f"[LLM_CALLER] Agent type: {type(agent).__name__}")

            # Call LLM
            started = time.perf_counter()
            try:
                response = agent.call_llm(
                    messages, task_monitor=task_monitor, thinking_visible=thinking_visible
                )
            except Exception as e:
                logger.error(f"[LLM_CALLER] Exception in agent.call_llm: {type(e).__name__}: {e}")
                logger.error(f"[LLM_CALLER] Full traceback:\n{traceback.format_exc()}")
                # Re-raise to be handled by outer try
                raise
            latency_ms = int((time.perf_counter() - started) * 1000)

            # DEBUG: Log response
            logger.debug(f"[LLM_CALLER] Response received: success={response.get('success')}")
            logger.debug(f"[LLM_CALLER] Response keys: {list(response.keys()) if isinstance(response, dict) else 'not a dict'}")

            # Get LLM description
            message_payload = response.get("message", {}) or {}
            llm_description = response.get("content", message_payload.get("content", ""))

            # Stop progress and show final status
            progress.stop()
            progress.print_final_status(replacement_message=llm_description)

            return response, latency_ms
        except Exception as e:
            # DEBUG: Log any exception
            logger.error(f"[LLM_CALLER] Exception during LLM call: {type(e).__name__}: {e}")
            logger.error(f"[LLM_CALLER] Full traceback:\n{traceback.format_exc()}")

            from swecli.core.debug import get_debug_logger
            get_debug_logger().log(
                "llm_call_error",
                "llm",
                error=str(e),
                traceback=traceback.format_exc(),
            )

            progress.stop()
            # Return error response
            return {
                "success": False,
                "error": str(e),
                "content": f"Error: {e}",
            }, 0
        finally:
            # Clear current monitor
            self._current_task_monitor = None

    def request_interrupt(self) -> bool:
        """Request interrupt of currently running LLM task.

        Returns:
            True if interrupt was requested, False if no task is running
        """
        from swecli.ui_textual.debug_logger import debug_log
        debug_log("LLMCaller", "request_interrupt called")
        debug_log("LLMCaller", f"_current_task_monitor={self._current_task_monitor}")

        if self._current_task_monitor is not None:
            self._current_task_monitor.request_interrupt()
            debug_log("LLMCaller", "Called task_monitor.request_interrupt()")
            return True
        debug_log("LLMCaller", "No active task monitor")
        return False
