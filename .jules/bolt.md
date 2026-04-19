## 2024-03-24 - Debouncing User Inputs Triggers
**Learning:** React component API calls tied directly to text inputs for search and file query functionalities must be debounced, as otherwise every single keystroke creates a network request, potentially creating race conditions on the response where a slower prior response overwrites a faster later one.
**Action:** When working on real-time search or autocomplete functions, always check if there is a proper debounce. Implement a standard `setTimeout` + `clearTimeout` cleanup wrapper if one doesn't exist.

## 2024-04-11 - React useEffect Dependency Array Optimization
**Learning:** Omission of a dependency array in `useEffect` (e.g. in `ToolCallMessage.tsx`) causes the hook to execute after *every* render. When such a hook performs DOM measurements (like `scrollHeight`) and sets state (`setExpandHeight`), it triggers further unnecessary renders and layout recalculations, drastically degrading performance especially in long lists like a chat log.
**Action:** Always ensure `useEffect` and similar hooks have appropriate dependency arrays to restrict their execution strictly to when their dependencies change.

## 2024-04-19 - Isolating High-Frequency Animations
**Learning:** `setInterval` states (like animation timers running at 100ms intervals) residing in high-level components (like `LandingPage` and `WelcomeScreen`) cause their entire component subtrees to re-render ten times a second. This leads to massive layout thrashing and poor responsiveness, especially when inputs are present.
**Action:** Always extract high-frequency local state updates (like spinners or timers) into their own isolated, leaf-level components using `React.memo()`. Keep state strictly co-located with the UI that depends on it.
