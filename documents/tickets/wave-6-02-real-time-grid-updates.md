# Wave 6.2 — Implement Real-Time Grid Updates (Jotai + SSE)

| Field | Value |
|-------|-------|
| **Wave** | 6 — Frontend: Real-time SSE, Config UI & Polish |
| **Seq** | 02 |
| **Estimate** | 2 hours |
| **Depends on** | 6.1 (SSE hook), 4.7 (thumbnail grid) |
| **Parallel** | No |

---

## Overview

Wire SSE events into the grid and TanStack Query cache so that file system changes are reflected in the UI in real-time. When a new file is created, it appears at the top of the grid. When a file is deleted, it disappears. When a file is modified, its metadata updates.

## Prerequisites

- SSE hook (6.1)
- Thumbnail grid with TanStack Query (4.3, 4.7)
- Jotai atoms for search and media state (5.2)

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 6 task 6.2, §3.3 SSE Event Format
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/store/
└── sse-atoms.ts                 # Updated: SSE event → Query cache invalidation
```

## Acceptance Criteria (Pass/Fail)

- [ ] `file_created` event → new item prepended to the media list (Query cache updated)
- [ ] `file_deleted` event → item removed from the media list
- [ ] `file_modified` event → item's metadata refreshed in Query cache
- [ ] `indexing_complete` event → media list fully refreshed (invalidate all pages)
- [ ] `lagged` event → full grid refresh triggered (too many missed events)
- [ ] Updates are efficient: single item changes don't refetch entire pages
- [ ] If search is active, new/deleted items update search results too
- [ ] Doesn't interrupt user browsing (no scroll position reset on updates)

## Implementation Notes

**Strategy: TanStack Query cache manipulation**

Rather than refetching pages from the server, manipulate the Query cache directly for individual item changes. For batch events (indexing_complete), invalidate and refetch.

```typescript
// In a top-level component or hook
import { useQueryClient } from '@tanstack/react-query';
import { useSse } from './use-sse';
import { fetchMediaItem } from '../api/media';

function useSseGridUpdates() {
  const queryClient = useQueryClient();

  useSse({
    onEvent: (event) => {
      switch (event.event) {
        case 'file_created': {
          // Fetch the full item and add it to the cache
          fetchMediaItem(event.data.id).then((item) => {
            queryClient.setQueryData(['media', 'list'], (oldData: any) => {
              if (!oldData?.pages) return oldData;
              
              const newPages = oldData.pages.map((page: any, index: number) => {
                if (index === 0) {
                  // Prepend to first page
                  return { ...page, data: [item, ...page.data] };
                }
                return page;
              });
              
              return { ...oldData, pages: newPages };
            });
          });
          break;
        }
        
        case 'file_deleted': {
          // Remove from cached pages
          queryClient.setQueryData(['media', 'list'], (oldData: any) => {
            if (!oldData?.pages) return oldData;
            
            const newPages = oldData.pages.map((page: any) => ({
              ...page,
              data: page.data.filter((item: any) => item.id !== event.data.id),
            }));
            
            return { ...oldData, pages: newPages };
          });
          break;
        }
        
        case 'file_modified': {
          // Invalidate the specific item
          queryClient.invalidateQueries({
            queryKey: ['media', 'item', event.data.id],
          });
          break;
        }
        
        case 'indexing_complete': {
          // Full re-fetch — indexing might have added many items
          queryClient.invalidateQueries({ queryKey: ['media', 'list'] });
          queryClient.invalidateQueries({ queryKey: ['search'] });
          break;
        }
        
        case 'lagged': {
          // Too many events missed — refresh everything
          queryClient.invalidateQueries({ queryKey: ['media', 'list'] });
          queryClient.invalidateQueries({ queryKey: ['search'] });
          break;
        }
      }
    },
  });
}
```

**Optimistic updates for new files:**
For `file_created`, the SSE event includes the full `MediaItem` data. We can insert it directly without an extra fetch:

```typescript
case 'file_created': {
  const newItem: MediaItem = event.data;
  queryClient.setQueryData(['media', 'list'], (oldData: any) => {
    if (!oldData?.pages) return oldData;
    const newPages = [...oldData.pages];
    newPages[0] = {
      ...newPages[0],
      data: [newItem, ...newPages[0].data],
      meta: {
        ...newPages[0].meta,
        total: (newPages[0].meta?.total ?? 0) + 1,
      },
    };
    return { ...oldData, pages: newPages };
  });
  break;
}
```

**Note on the SSE event data:** Per §3.3, `file_created` events include a lightweight MediaItem with thumbnail_url. This is sufficient to display in the grid immediately. For detail view, the full data is fetched on-demand.

**Grid notification indicator:**
Optionally show a subtle indicator when new files arrive while the user is scrolled down:
```tsx
// "New files available" toast/banner
{hasNewFiles && (
  <div className="sticky top-0 z-10 bg-blue-600 text-white text-sm px-4 py-2 text-center cursor-pointer">
    New files available — scroll to top
  </div>
)}
```

## Test Strategy

Test the cache manipulation logic by mocking the SSE events and verifying the Query cache:

```typescript
import { renderHook, act } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

describe('useSseGridUpdates', () => {
  it('prepends new item on file_created', async () => {
    // Set up initial cache with a page of items
    // Fire file_created SSE event
    // Verify new item appears at index 0 of first page
  });

  it('removes item on file_deleted', async () => {
    // Fire file_deleted event
    // Verify item removed from all pages
  });

  it('invalidates on indexing_complete', async () => {
    // Fire indexing_complete
    // Verify queries are invalidated
  });
});
```
