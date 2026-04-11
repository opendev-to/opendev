## 2024-03-24 - Debouncing User Inputs Triggers
**Learning:** React component API calls tied directly to text inputs for search and file query functionalities must be debounced, as otherwise every single keystroke creates a network request, potentially creating race conditions on the response where a slower prior response overwrites a faster later one.
**Action:** When working on real-time search or autocomplete functions, always check if there is a proper debounce. Implement a standard `setTimeout` + `clearTimeout` cleanup wrapper if one doesn't exist.
