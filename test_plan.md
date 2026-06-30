1. **Optimize Math.min in sortedSecondary in `buildGraph.ts`**
   - The current code:
     ```typescript
     const sortedSecondary = [...secondaryComps.entries()].sort((a, b) => {
       const minA = Math.min(...a[1].map(n => getNodeTimestamp(n.data)));
       const minB = Math.min(...b[1].map(n => getNodeTimestamp(n.data)));
       return minA - minB;
     });
     ```
   - This recalculates the `minTime` for each secondary component O(N log N) times during sorting, which causes many array allocations, spread operations, and redundant calculations.
   - We already compute the minimum time for each component in `compMinTime`. So we can use that directly:
     ```typescript
     const sortedSecondary = [...secondaryComps.entries()].sort((a, b) => {
       const minA = compMinTime.get(a[0]) ?? 0;
       const minB = compMinTime.get(b[0]) ?? 0;
       return minA - minB;
     });
     ```

2. **Add entry to Bolt Journal**
   - File: `.jules/bolt.md`
   - Entry:
     ```markdown
     ## 2024-07-30 - O(N log N) Array Allocations in Sort Comparators
     **Learning:** Computing minimums of component arrays inside a `sort` comparator using `Math.min(...array.map(...))` creates massive performance bottlenecks and O(N log N) redundant array allocations and spread operations. For large components, it can even trigger maximum call stack size exceeded errors.
     **Action:** Always precompute aggregate values like minimum times before sorting, or rely on already-cached values (like `compMinTime`) for O(1) lookups inside the sort comparator.
     ```

3. **Verify**
   - Run frontend tests: `cd web-ui && node --experimental-strip-types --test`.

4. **Pre-commit step**
   - Complete pre-commit steps to ensure proper testing, verification, review, and reflection are done.

5. **Submit**
   - Submit PR with title "⚡ Bolt: O(1) compMinTime lookups in layoutGraph sort".
