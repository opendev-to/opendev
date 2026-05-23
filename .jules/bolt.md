## 2024-03-24 - Debouncing User Inputs Triggers
**Learning:** React component API calls tied directly to text inputs for search and file query functionalities must be debounced, as otherwise every single keystroke creates a network request, potentially creating race conditions on the response where a slower prior response overwrites a faster later one.
**Action:** When working on real-time search or autocomplete functions, always check if there is a proper debounce. Implement a standard `setTimeout` + `clearTimeout` cleanup wrapper if one doesn't exist.

## 2024-04-11 - React useEffect Dependency Array Optimization
**Learning:** Omission of a dependency array in `useEffect` (e.g. in `ToolCallMessage.tsx`) causes the hook to execute after *every* render. When such a hook performs DOM measurements (like `scrollHeight`) and sets state (`setExpandHeight`), it triggers further unnecessary renders and layout recalculations, drastically degrading performance especially in long lists like a chat log.
**Action:** Always ensure `useEffect` and similar hooks have appropriate dependency arrays to restrict their execution strictly to when their dependencies change.

## 2024-04-19 - Isolating High-Frequency Animations
**Learning:** `setInterval` states (like animation timers running at 100ms intervals) residing in high-level components (like `LandingPage` and `WelcomeScreen`) cause their entire component subtrees to re-render ten times a second. This leads to massive layout thrashing and poor responsiveness, especially when inputs are present.
**Action:** Always extract high-frequency local state updates (like spinners or timers) into their own isolated, leaf-level components using `React.memo()`. Keep state strictly co-located with the UI that depends on it.
## 2024-05-13 - Preventing Layout Thrashing on Hidden Elements
**Learning:** React components that stream content updates (like `ToolCallMessage` and `ThinkingBlock`) can cause severe layout thrashing if they synchronously measure the DOM (e.g., `scrollHeight`) inside a `useEffect` on every update, *even when collapsed*. Furthermore, trying to optimize this by putting expressions like `isExpanded ? message : null` into the dependency array breaks React linting rules (`react-hooks/exhaustive-deps`).
**Action:** Always guard expensive DOM measurements with a visibility check (`if (isExpanded)`) inside the effect itself. Keep dependency arrays simple and exhaustive (`[isExpanded, message]`).

## 2024-05-13 - TOCTOU Vulnerability in File Initialization
**Learning:** `Path::exists()` followed by `fs::write()` is a classic Time-Of-Check to Time-Of-Use (TOCTOU) race condition vulnerability, which manifests as flaky test failures in highly concurrent environments (like tests hitting the same file path).
**Action:** Always use `OpenOptions::new().create_new(true).write(true).open()` to safely initialize a file only if it doesn't already exist.

## 2024-05-15 - Referential Stability of Component Props
**Learning:** Passing inline objects to props (like the `components` prop of `ReactMarkdown`) in functional components breaks referential stability, causing the component and its entire subtree to re-render unnecessarily on every parent render.
**Action:** Always extract static configuration objects and functions that don't depend on component state outside of the functional component definition.

## 2024-05-23 - Debouncing Local Arrays Degrades Perceived Performance
**Learning:** While debouncing is essential for reducing network API calls or heavy backend processing when a user types in a search box, applying `useDebounce` to filter small, local, in-memory arrays (like `mockRepositories` or pre-loaded state) artificially introduces UI latency (e.g., 300ms delay before results appear). This feels sluggish to the user and is a net performance regression compared to executing the fast, synchronous filter loop immediately.
**Action:** Never debounce synchronous array filtering unless the array is massively large and blocking the main thread. If it's a cold path or standard list, rely on `useMemo` to cache the results and hoist expensive operations (like `.toLowerCase()`) outside the loop instead of debouncing.
