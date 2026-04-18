## 2024-03-24 - Debouncing User Inputs Triggers
**Learning:** React component API calls tied directly to text inputs for search and file query functionalities must be debounced, as otherwise every single keystroke creates a network request, potentially creating race conditions on the response where a slower prior response overwrites a faster later one.
**Action:** When working on real-time search or autocomplete functions, always check if there is a proper debounce. Implement a standard `setTimeout` + `clearTimeout` cleanup wrapper if one doesn't exist.

## 2024-04-11 - React useEffect Dependency Array Optimization
**Learning:** Omission of a dependency array in `useEffect` (e.g. in `ToolCallMessage.tsx`) causes the hook to execute after *every* render. When such a hook performs DOM measurements (like `scrollHeight`) and sets state (`setExpandHeight`), it triggers further unnecessary renders and layout recalculations, drastically degrading performance especially in long lists like a chat log.
**Action:** Always ensure `useEffect` and similar hooks have appropriate dependency arrays to restrict their execution strictly to when their dependencies change.

## 2024-04-19 - Isolating High-Frequency Animations
**Learning:** `setInterval` states (like animation timers running at 100ms intervals) residing in high-level components (like `LandingPage` and `WelcomeScreen`) cause their entire component subtrees to re-render ten times a second. This leads to massive layout thrashing and poor responsiveness, especially when inputs are present.
**Action:** Always extract high-frequency local state updates (like spinners or timers) into their own isolated, leaf-level components using `React.memo()`. Keep state strictly co-located with the UI that depends on it.
## 2024-05-08 - Extracting High-Frequency Timer States
**Learning:** Having `setInterval` and `useState` tracking elapsed time inside high-level components or list items (like `SubagentNode` and `ActiveToolRow`) causes unnecessary re-renders of the entire item on every tick. This results in poor performance and layout thrashing.
**Action:** Extract the elapsed time timer state into an isolated `ElapsedTimeDisplay` component wrapped in `React.memo()`. This ensures only the tiny text node re-renders on every tick.

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

## 2024-05-24 - Array Filtering Operations in React Renders
**Learning:** Performing array filtering with repeated inner `.toLowerCase()` calls during every render cycle (even outside `useEffect`) introduces unnecessary memory allocations and O(N) redundant string operations. This causes measurable UI lag when handling large sets of data, especially when state values updates frequently (e.g. searching/typing).
**Action:** Always wrap expensive synchronous array filtering in `useMemo`, and hoist repetitive value normalization (like query.toLowerCase()) outside of the filtering loop.

## 2024-05-24 - Inline Component Array Filtering
**Learning:** Performing array filtering with repeated inner `.toLowerCase()` calls during every render cycle inside functional components without `useMemo` (like `CommandPalette` and `NewSessionModal`) introduces unnecessary memory allocations and O(N) redundant string operations. This causes measurable UI lag when handling large sets of data, especially when state values updates frequently.
**Action:** Always wrap expensive synchronous array filtering in `useMemo`, and hoist repetitive value normalization (like `query.toLowerCase()`) outside of the filtering loop.

## 2024-05-25 - Rules of Hooks and JSX IIFEs
**Learning:** Attempting to apply `useMemo` optimizations directly inside an Immediately Invoked Function Expression (IIFE) within JSX violates React's Rules of Hooks. Hooks must be placed at the top level of the component body, never inside nested functions or IIFEs.
**Action:** When extracting expensive logic (like array filtering) from a JSX IIFE into a memoized value, ensure the `useMemo` hook is hoisted to the top level of the component, and only the resulting memoized value is used within the JSX.

## 2024-04-18 - Replacing synchronous std::fs operations with tokio::fs in async contexts
**Learning:** In the `SymbolCache` implementation in `crates/opendev-tools-lsp/src/cache.rs`, using synchronous `std::fs` operations (e.g., `read_to_string`, `write`, `create_dir_all`, `remove_dir_all`) inside async functions blocks the async executor thread. This is a common performance bottleneck in Rust async applications, as it prevents other async tasks from making progress on the thread handling the I/O.
**Action:** Always verify if `std::fs` is being called within an `async fn` or an executor's context. When identifying such usage, refactor to use `tokio::fs` equivalents (e.g., `tokio::fs::read_to_string().await`) to ensure non-blocking file I/O operations and improve overall application concurrency.
