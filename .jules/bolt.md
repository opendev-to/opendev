# Bolt's Journal

## 2024-05-30 - Added Debouncing to File Mention Search
**Learning:** In the `InputBox.tsx` component, the file mention search (`@` functionality) was executing an API request (`apiClient.listFiles`) on every single keystroke. This causes excessive network traffic and can lead to race conditions where older requests overwrite newer ones.
**Action:** Implemented a standard 300ms debounce using `setTimeout` inside the `useEffect` hook that watches the `mentionQuery`. This is a crucial pattern for any real-time search or autocomplete feature in this React application to prevent performance bottlenecks and ensure smooth user experience.
