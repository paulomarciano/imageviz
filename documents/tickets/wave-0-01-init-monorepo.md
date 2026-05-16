# Wave 0.1 — Initialize Monorepo Structure

| Field | Value |
|-------|-------|
| **Wave** | 0 — Project Scaffolding & CI |
| **Seq** | 01 |
| **Estimate** | 30 minutes |
| **Depends on** | Nothing |
| **Parallel** | No |

---

## Overview

Create the top-level monorepo directory structure matching §12 of the development plan. This establishes the canonical layout that all future tasks will populate.

## Prerequisites

- Git repository initialized at `imageviz/` root
- No existing `backend/` or `frontend/` directories

## Reference Files

- `documents/plans/development-plan.md` — §12 Project Structure (lines 686–826)

## Deliverables

The following directories must exist:

```
imageviz/
├── .gitignore
├── README.md                          # Minimal project README (name + one-liner)
├── .github/
│   └── workflows/                     # Empty dir (populated in 0.9)
├── documents/
│   └── plans/
│       └── development-plan.md         # Already exists
├── test-fixtures/                      # Empty dir (populated in subsequent waves)
├── backend/                            # Empty dir (scaffolded in 0.2)
├── frontend/                           # Empty dir (scaffolded in 0.3)
└── scripts/                            # Empty dir (populated in 7.8)
```

## Acceptance Criteria (Pass/Fail)

- [x] `backend/` directory exists
- [x] `frontend/` directory exists
- [x] `.github/workflows/` directory exists
- [x] `test-fixtures/` directory exists
- [x] `scripts/` directory exists
- [x] `documents/plans/` directory exists
- [x] `.gitignore` exists with appropriate entries for Rust (`target/`), Node (`node_modules/`, `dist/`), and OS files
- [x] `README.md` exists with project name "ImageViz" and a one-line description

## Implementation Notes

1. **`.gitignore`** should cover:
   - Rust: `target/`, `*.rs.bk`
   - Node: `node_modules/`, `dist/`, `.env`
   - OS: `.DS_Store`, `Thumbs.db`
   - IDE: `.vscode/`, `.idea/`
2. **`README.md`** should be minimal — just the project name and tagline. It will be expanded in Wave 7.10.
3. Do NOT create any source files inside `backend/` or `frontend/` — those are scaffolded in 0.2 and 0.3 respectively.

## Test Strategy

- Verify directory structure exists: `ls backend/ frontend/ .github/workflows/ test-fixtures/ scripts/`
- Verify `.gitignore` is not empty: `test -s .gitignore`
- Verify `README.md` exists and contains "ImageViz"
