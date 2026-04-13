## 2024-03-24 - Debouncing User Inputs Triggers
**Learning:** React component API calls tied directly to text inputs for search and file query functionalities must be debounced, as otherwise every single keystroke creates a network request, potentially creating race conditions on the response where a slower prior response overwrites a faster later one.
**Action:** When working on real-time search or autocomplete functions, always check if there is a proper debounce. Implement a standard `setTimeout` + `clearTimeout` cleanup wrapper if one doesn't exist.

## 2024-04-11 - React useEffect Dependency Array Optimization
**Learning:** Omission of a dependency array in `useEffect` (e.g. in `ToolCallMessage.tsx`) causes the hook to execute after *every* render. When such a hook performs DOM measurements (like `scrollHeight`) and sets state (`setExpandHeight`), it triggers further unnecessary renders and layout recalculations, drastically degrading performance especially in long lists like a chat log.
**Action:** Always ensure `useEffect` and similar hooks have appropriate dependency arrays to restrict their execution strictly to when their dependencies change.

## 2024-05-18 - Isolate High-Frequency State Updates
**Learning:** In React, keeping high-frequency state updates (like a 100ms interval for an animation) in a top-level or heavy component like `LandingPage` causes the entire component and its children to re-render constantly. This can lead to noticeable input latency (e.g., in a textarea) and unnecessary layout thrashing.
**Action:** Always extract high-frequency animations and their driving state into independent, lightweight "leaf" components (e.g., `HaloSpinner`) so their continuous re-renders do not affect the main UI thread or heavy sibling components.
