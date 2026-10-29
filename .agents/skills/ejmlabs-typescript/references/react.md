# State, hooks and data

## Who owns which state

| State                                                         | Owner                                                                |
| ------------------------------------------------------------- | -------------------------------------------------------------------- |
| Unsaved form input, open menus, hover, local toggles          | `useState` or `useReducer` in the component                          |
| Server records                                                | TanStack Query, through the feature's `queries.ts`                   |
| A view someone can share or reload (filters, sort, page, tab) | URL search params, through the router                                |
| Theme                                                         | `useTheme` from `@ejm/ui/hooks/useTheme`; never a second theme store |
| App state several features read                               | A hook in the app shell, passed down as its state object             |

- One owner per piece of state. Never mirror it into a second store or a context copy.
- Never add a global state library for data the query cache already owns.
- Keep caches scoped to the signed-in account and workspace. Clear them when access changes.

## Hooks

- A hook that returns an object exports its type next to it:
  `export type ThingState = ReturnType<typeof useThing>;`. Never write `ReturnType<...>` inline
  in props.
- A hook that needs another hook's state takes that state object, not loose setters and refs.
- Each hook owns its own `reset()`. Callers compose resets; they never reach into another hook.
- Reusable and leaf components take explicit props. Only a hook's own sub-views take its state
  object as one prop.

## Effects

- Use an effect only to sync with a system outside React: a subscription, an observer, a timer,
  `matchMedia`, a DOM or platform API. Return the cleanup.
- Never use an effect to derive state, copy props, or chain state updates. Compute it in render.
- An effect that hides content until something fires (an `IntersectionObserver` reveal) also sets
  the final state on a timer, so content can never stay hidden.
- Theme hooks follow `prefers-color-scheme` through `matchMedia` while the visitor has not chosen.

## Fetching

- Requests go through the feature's `api.ts` and one shared HTTP client. The client owns decoding,
  errors, cancellation and session expiry. No component-level `fetch` wrappers.
- Query keys include every input that selects the data. Mutations update or invalidate every
  query they affect.
- Cancellation stays cancellation, never an error message. A failed request keeps unsent input.
- The server is the authority. Client checks give faster feedback; they never replace the server.
- A list that grows is filtered, sorted and paged by the server. Never filter a paged list again
  on the client.
- Show loading, empty, error and stale states. A missing value is shown as unknown, never as zero.

## Rendering cost

- Lazy load heavy routes, charts and editors (`React.lazy` or the route's `load`).
- Bound list sizes and chart data. Batch lookups instead of nested scans that grow with the data.
- Profile before adding memoization, and keep it only when it removes a measured repeated cost.
