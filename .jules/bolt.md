## 2024-03-24 - Debouncing User Inputs Triggers
**Learning:** React component API calls tied directly to text inputs for search and file query functionalities must be debounced, as otherwise every single keystroke creates a network request, potentially creating race conditions on the response where a slower prior response overwrites a faster later one.
**Action:** When working on real-time search or autocomplete functions, always check if there is a proper debounce. Implement a standard `setTimeout` + `clearTimeout` cleanup wrapper if one doesn't exist.

## 2024-04-11 - React useEffect Dependency Array Optimization
**Learning:** Omission of a dependency array in `useEffect` (e.g. in `ToolCallMessage.tsx`) causes the hook to execute after *every* render. When such a hook performs DOM measurements (like `scrollHeight`) and sets state (`setExpandHeight`), it triggers further unnecessary renders and layout recalculations, drastically degrading performance especially in long lists like a chat log.
**Action:** Always ensure `useEffect` and similar hooks have appropriate dependency arrays to restrict their execution strictly to when their dependencies change.
## 2024-04-15 - React Component Re-render Optimization
**Learning:** In React functional components, high-frequency state updates like animation intervals (`setInterval` + `useState`) can cause layout thrashing and unnecessary deep re-renders when placed inside large parent components that have complex children.
**Action:** Isolate high-frequency state updates into dedicated child components, and wrap them with `React.memo` if they don't depend on parent props. This keeps the state localized, preventing parent and sibling components from re-rendering on every interval tick.
