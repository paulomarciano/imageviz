# Wave 0.6 — Configure ESLint + Prettier + Rustfmt + Clippy

| Field | Value |
|-------|-------|
| **Wave** | 0 — Project Scaffolding & CI |
| **Seq** | 06 |
| **Estimate** | 30 minutes |
| **Depends on** | 0.2 (Rust backend), 0.3 (React frontend) |
| **Parallel** | Can run in parallel with 0.4, 0.5 |

---

## Overview

Set up linting and formatting tooling for both the Rust backend and TypeScript frontend. Ensure consistent code quality across the monorepo.

## Prerequisites

- Backend `Cargo.toml` exists (from 0.2)
- Frontend `package.json` exists (from 0.3)

## Reference Files

- `documents/plans/development-plan.md` — §2 Tech Stack (ESLint, Prettier, rustfmt, clippy)
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/
├── rustfmt.toml                    # Rust formatter config (create/update)
└── .cargo/
    └── config.toml                 # Cargo config (update with clippy settings)

frontend/
├── .eslintrc.cjs                   # ESLint config (flat config for v9)
├── .prettierrc                     # Prettier config
└── package.json                    # Updated with lint scripts
```

## Acceptance Criteria (Pass/Fail)

**Rust:**
- [ ] `cargo fmt --check` passes with no formatting violations
- [ ] `cargo clippy -- -D warnings` passes with no warnings
- [ ] `rustfmt.toml` specifies: `max_width = 100`, `tab_spaces = 4`, `edition = "2024"`

**Frontend:**
- [ ] `npx eslint .` passes with no errors
- [ ] `npx prettier --check .` passes with no formatting violations
- [ ] `package.json` has scripts: `"lint": "eslint ."`, `"format": "prettier --write ."`, `"format:check": "prettier --check ."`
- [ ] `.prettierrc` specifies: `singleQuote: true`, `trailingComma: "all"`, `semi: true`

## Implementation Notes

**ESLint v9 (flat config)** — Use `eslint.config.mjs` (ESLint v9 dropped `.eslintrc`):
```javascript
import js from '@eslint/js';
import tseslint from 'typescript-eslint';

export default tseslint.config(
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    rules: {
      '@typescript-eslint/no-unused-vars': 'error',
      '@typescript-eslint/explicit-function-return-type': 'off',
    },
  },
);
```

**rustfmt.toml:**
```toml
max_width = 100
tab_spaces = 4
edition = "2024"
newline_style = "Unix"
use_small_heuristics = "Max"
```

**`.cargo/config.toml`** — add clippy warnings-as-errors:
```toml
[target.x86_64-unknown-linux-gnu]
rustflags = ["-D", "warnings"]
```

## Test Strategy

- `cargo fmt --check && cargo clippy -- -D warnings` — must exit 0
- `npx eslint . && npx prettier --check .` — must exit 0
- These commands will be added to CI in Wave 0.9
