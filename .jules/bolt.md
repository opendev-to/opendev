
## 2024-04-05 - File Mention Debouncing
**Learning:** The React frontend triggers `apiClient.listFiles` on every keystroke when the file mention menu is open, leading to excessive backend load and potential race conditions in list rendering.
**Action:** Implement a 300ms `setTimeout` debounce in the `useEffect` hook, storing the timeout ID and returning `clearTimeout` in the cleanup function to prevent stale requests from resolving out of order.
