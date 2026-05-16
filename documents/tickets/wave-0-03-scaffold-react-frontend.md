# Wave 0.3 — Scaffold React Frontend with Vite + TypeScript + Tailwind

| Field | Value |
|-------|-------|
| **Wave** | 0 — Project Scaffolding & CI |
| **Seq** | 03 |
| **Estimate** | 45 minutes |
| **Depends on** | 0.1 (monorepo structure) |
| **Parallel** | Can run in parallel with 0.2 |

---

## Overview

Initialize the React frontend project using Vite 8 with TypeScript strict mode and Tailwind CSS 4. Create a minimal app that renders a Tailwind-styled page. This establishes the frontend toolchain and project layout for all frontend waves.

## Prerequisites

- Node.js installed (recent LTS)
- `npm` available
- `frontend/` directory exists (from 0.1)

## Reference Files

- `documents/plans/development-plan.md` — §2 (Tech Stack), §13 Appendix (package.json dependencies), §12 (project structure)
- `.opencode/context/core/standards/code-quality.md` — functional patterns, immutability

## Deliverables

```
frontend/
├── package.json                       # All deps from §13
├── tsconfig.json                      # Strict mode, path aliases
├── vite.config.ts                     # Vite 8 config with React plugin + Tailwind
├── tailwind.config.ts                 # Tailwind CSS v4 config
├── index.html                         # Entry HTML
└── src/
    ├── main.tsx                       # React entry point
    └── App.tsx                        # Root component with Tailwind styling
```

## Acceptance Criteria (Pass/Fail)

- [ ] `npm install` completes without errors
- [ ] `npm run dev` starts Vite dev server and shows a page
- [ ] The page renders with Tailwind CSS styling applied (visible background/text colors)
- [ ] `npx tsc --noEmit` passes (no TypeScript errors) — strict mode enabled
- [ ] `package.json` includes all dependencies from §13:
  - Runtime: `react ^19.0`, `react-dom ^19.0`, `react-virtuoso ^4.18`, `react-dnd ^16.0`, `react-dnd-html5-backend ^16.0`, `@tanstack/react-query ^5.100`, `jotai ^2.20`
  - Dev: `@vitejs/plugin-react ^4.5`, `vite ^8.0`, `typescript ^5.8`, `tailwindcss ^4.3`, `@tailwindcss/vite ^4.3`
  - Test (dev): `vitest ^3.1`, `@testing-library/react ^16.3`, `@testing-library/jest-dom ^6.6`, `@testing-library/user-event ^14.6`
  - Lint (dev): `eslint ^9.0`, `prettier ^3.5`

## Implementation Notes

1. **Scaffold with Vite**:
   ```bash
   cd frontend
   npm create vite@8 . -- --template react-ts
   ```
   Then add additional dependencies manually via `npm install`.

2. **Tailwind CSS v4 setup** — Tailwind v4 uses the Vite plugin directly:
   ```typescript
   // vite.config.ts
   import { defineConfig } from 'vite';
   import react from '@vitejs/plugin-react';
   import tailwindcss from '@tailwindcss/vite';

   export default defineConfig({
     plugins: [react(), tailwindcss()],
   });
   ```

3. **App.tsx** should include at minimum a Tailwind-styled element to verify the pipeline:
   ```tsx
   function App() {
     return (
       <div className="min-h-screen bg-gray-900 text-white flex items-center justify-center">
         <h1 className="text-2xl font-bold">ImageViz</h1>
       </div>
     );
   }
   ```

4. **`tsconfig.json`** — set `"strict": true`, and configure path aliases if desired (e.g., `@/` → `src/`).

5. **Do NOT** create any API clients, hooks, or business logic yet — those come in Wave 4.

## Test Strategy

- Manual: `npm run dev` then open browser to verify Tailwind renders
- Manual: `npx tsc --noEmit` verifies type checking
- No automated tests yet — Wave 0.8 adds the first frontend smoke test

## External Docs

During implementation, use **ExternalScout** to fetch current docs for:
- `@tailwindcss/vite` — Tailwind CSS v4 Vite integration (v4 API differs from v3)
- `@vitejs/plugin-react` — Vite React plugin configuration
