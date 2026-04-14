## 2024-03-24 - Debouncing User Inputs Triggers
**Learning:** React component API calls tied directly to text inputs for search and file query functionalities must be debounced, as otherwise every single keystroke creates a network request, potentially creating race conditions on the response where a slower prior response overwrites a faster later one.
**Action:** When working on real-time search or autocomplete functions, always check if there is a proper debounce. Implement a standard `setTimeout` + `clearTimeout` cleanup wrapper if one doesn't exist.

## 2024-04-11 - React useEffect Dependency Array Optimization
**Learning:** Omission of a dependency array in `useEffect` (e.g. in `ToolCallMessage.tsx`) causes the hook to execute after *every* render. When such a hook performs DOM measurements (like `scrollHeight`) and sets state (`setExpandHeight`), it triggers further unnecessary renders and layout recalculations, drastically degrading performance especially in long lists like a chat log.
**Action:** Always ensure `useEffect` and similar hooks have appropriate dependency arrays to restrict their execution strictly to when their dependencies change.
## 2024-05-24 - [Isolate High-Frequency State Updates]
**Learning:** React components containing high-frequency intervals (like a `setInterval` running every 100ms for an animation) will trigger a full re-render of the component and all its children on every tick. When this pattern is used in large, complex components like `LandingPage` or `WelcomeScreen`, it causes severe layout thrashing and performance degradation across the entire UI.
**Action:** Always extract high-frequency state updates (like animations or progress bars) into isolated, independent components. Wrap these child components in `React.memo` if they accept props, or let them manage their own internal state entirely, to ensure that only the smallest necessary subtree re-renders during the high-frequency updates.
