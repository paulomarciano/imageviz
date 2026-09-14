# Semantic Search Investigation Plan

> **Status**: Draft — requires validation  
> **Created**: 2026-06-16  
> **Target release**: v0.8.0 or v1.0.0  
> **Dependencies**: None (investigation only)

---

## 1. Overview

Add semantic (vector-based) image search to ImageViz using Qwen3-VL embeddings. The vision-language model generates dense vector representations from images at indexing time, enabling natural-language and visual-similarity queries against the media library. This runs alongside — not replacing — the existing Tantivy keyword search.

### Key Design Decisions (Already Decided)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Embedding model | Qwen3-VL-Embedding-2B (Apache 2.0) | State-of-the-art multimodal embedding, 2B params, open weights |
| Initial provider | OpenRouter API | Fastest path to production, no GPU provisioning |
| Future self-hosting | vLLM or llama.cpp on local GPU/CPU | Cost savings at scale, data privacy |
| UI strategy | Separate semantic search UI (independent from main browse/search) | Different interaction model, avoids cluttering existing UI |
| Vector store | TBD (see §4) | — |

---

## 2. Open Questions Requiring Investigation

These questions **must** be answered before implementation begins. Each has implications for architecture, cost, and user experience.

### 🔴 Q1 — CPU Embedding Performance (Critical Path)

> **How long does Qwen3-VL-Embedding-2B take to embed a single 1MP image on CPU?**

**Why this matters**: If CPU inference is fast enough (≤30s/image), self-hosting on the user's machine is viable. If it's minutes per image, a GPU or API provider is required for acceptable UX.

**Research findings so far**:
| Data-point | Context | Time |
|------------|---------|------|
| Qwen3-VL-2B-Instruct forward pass | RTX 5090, 2× 256×256 images | ~2.5s total |
| Vision encoder only (image processing) | RTX 5090, 2× 256×256 (~67K px) | ~2.35s (94% of total) |
| LLM backbone only (no images) | RTX 5090 | ~0.15s |
| L4 GPU benchmark (Flickr30kI2TRetrieval) | L4 (~30 TFLOPS FP16) | p50 = 4.3s |
| 1MP vs 256×256 pixel ratio | — | ~15× more pixels/patches |

**CPU extrapolation** (rough, needs validation):
- RTX 5090: ~100 TFLOPS FP16. Modern desktop CPU: ~0.3–0.5 TFLOPS FP32.
- Vision encoder GPU→CPU slowdown: 200–300× for FP16 workloads.
- With Q4_K_M quantization (GGUF): 4× memory bandwidth reduction, ~30–50× effective slowdown.
- **Estimated**: **30–120 seconds per 1MP image** on a modern 8+ core CPU (Ryzen 7/9 class).

**Validation needed**:
1. Run llama.cpp with Qwen3-VL-Embedding-2B GGUF (Q4_K_M) on a modern desktop CPU
2. Measure: load model → load 1MP PNG → encode → wall-clock time
3. Test with 10–20 diverse images, report p50, p95, p99
4. Test with smaller resolutions: 512px, 768px (trade-off: speed vs quality)

---

### 🔴 Q2 — Embedding API Provider Landscape

> **What are the viable API providers for Qwen3-VL-Embedding, and what are their pricing, latency, and availability characteristics?**

**Research findings so far**:

| Provider | Qwen3-VL-Embedding-2B? | Pricing Model | Notes |
|----------|------------------------|---------------|-------|
| **OpenRouter** | ❓ Not yet listed | Per-token (Qwen3-Embedding-8B text: $0.01/M input) | Qwen VL models available (Instruct variants); Embedding variant may need request |
| **Together AI** | ✅ Listed on HF | Per-token (varies by model) | Strong VL infrastructure, high throughput |
| **Fireworks AI** | ✅ Listed on HF | Per-token | Known for low-latency inference |
| **Replicate** | ✅ Listed on HF | Per-second GPU rental | Predictable cost, cold starts possible |
| **FriendliAI** | ✅ Listed explicitly | Not yet researched | Claims "unmatched speed and reliability" |
| **Alibaba DashScope** | ✅ First-party | Per-token (official Qwen pricing) | Native support, may have best pricing |
| **HF Inference Endpoints** | ✅ (self-deploy) | Per-hour GPU ($1–3/hr) | Full control, no per-call markup |

**OpenRouter specifics** (important because it's the initial choice):
- Currently serves Qwen3-VL **generation** models (8B/32B/235B Instruct versions)
- Does NOT list Qwen3-VL-**Embedding**-2B specifically
- Qwen3-Embedding-8B (text-only) is listed at $0.01/M input tokens, $0.00 output
- **Action required**: Verify with OpenRouter whether `Qwen3-VL-Embedding-2B` is on their roadmap, or if the Instruct variant can be used for embedding extraction via their API

**Validation needed**:
1. Request model availability from OpenRouter support / check their model request process
2. Benchmark 3+ providers with identical 10-image test set: latency (p50, p95), cost per image, embedding quality consistency
3. Test embedding API compatibility: OpenAI-compatible embeddings endpoint vs custom format
4. Evaluate cold start latency for Replicate / serverless options
5. Research Alibaba DashScope pricing for Qwen3-VL-Embedding-2B specifically

---

### 🟡 Q3 — OpenRouter Self-Hosted Transition Path

> **What is the migration path from OpenRouter to self-hosted Qwen3-VL-Embedding?**

**Why this matters**: The architecture must not couple tightly to OpenRouter. Changing providers or self-hosting should require only configuration changes, not code rewrites.

**Research findings so far**:
- OpenRouter's embeddings API is OpenAI-compatible (`POST /embeddings` with `model`, `input`, `encoding_format`)
- Self-hosting options exist: vLLM (native pooling runner), SGLang, llama.cpp
- Community project: `philmcginty/qwen3-vl-embedding-server` — OpenAI-compatible wrapper
- llama.cpp discussion #19516 requests Qwen3-VL-Embedding support (Feb 2026)

**Validation needed**:
1. Test llama.cpp GGUF Qwen3-VL-Embedding-2B with multi-modal embedding support (check status of PR #19516)
2. Test vLLM with `runner="pooling"` and Qwen3-VL-Embedding-2B — confirm OpenAI-compatible embeddings endpoint
3. Verify embedding consistency: same image → same vector across OpenRouter vs self-hosted (within floating-point tolerance)
4. Benchmark self-hosted GPU options: consumer (RTX 3060 12GB, RTX 4090 24GB) vs cloud (L4, A10G)

---

### 🟡 Q4 — Vector Store Selection

> **Which vector database should store the embeddings?**

**Constraints**:
- Must work embedded (no separate server process) — same philosophy as SQLite + Tantivy
- Must handle 100K–1M vectors (2048 dimensions) efficiently
- Must support cosine similarity search with metadata filtering
- Rust-native or with good Rust bindings

**Candidates to investigate**:

| Candidate | Type | Pros | Cons |
|-----------|------|------|------|
| **LanceDB** | Embedded, columnar | Rust-native, no server, disk-based, fast ANN | Newer project, smaller community |
| **Qdrant** | Embedded mode | Production-grade, rich filtering, quantization | Heavier dependency, embedded mode is newer |
| **SQLite + sqlite-vec** | Extension | Same DB, zero new deps, simple | No ANN (exact search only), ~500ms for 100K vectors |
| **Faiss** (via Rust bindings) | Library | Industry standard, battle-tested | C++ FFI complexity, index rebuilding |
| **Annoy** (via Rust bindings) | Library | Minimal memory, fast | Read-only after build, no incremental updates |
| **pgvector** | DB extension | If PostgreSQL is added, it's a clean option | Requires PostgreSQL (new dependency) |
| **usearch** | Library | SIMD-optimized, Rust bindings | Smaller community |

**Validation needed**:
1. Benchmark LanceDB vs sqlite-vec vs Qdrant (embedded) with 100K × 2048-dim vectors on commodity hardware
2. Measure: indexing time (bulk + incremental single insert), query latency (p50/p95), memory usage, disk usage
3. Test metadata filtering (combine vector search with SQLite-style filters on `mime_type`, date range)
4. Evaluate quantization options (product quantization, scalar quantization) for storage reduction

---

### 🟢 Q5 — Embedding Quality at Different Resolutions

> **How does image resolution affect embedding quality for ComfyUI-generated images?**

**Why this matters**: If 512px embeddings are within 95% of 1MP quality, bandwidth/storage/compute savings are significant.

**Research findings so far**:
- Qwen3-VL-Embedding processes images through a vision encoder (ViT-based)
- Higher resolution → more visual detail captured → better embeddings
- For AI-generated images (ComfyUI), visual semantics are often in composition and style, which survive downscaling well

**Validation needed**:
1. Embed 100+ ComfyUI images at 256px, 512px, 768px, 1MP (original)
2. For each resolution: run same-collection retrieval, measure precision@10, recall@10 vs 1MP baseline
3. Determine "good enough" threshold (e.g., 512px if recall@10 ≥ 0.95 of full-res)
4. Document resolution-to-quality curve as a user-configurable trade-off

---

### 🟢 Q6 — UX Design for Semantic Search

> **What should the semantic search UI look like, and how does it integrate with the existing application?**

**Why this matters**: The user explicitly wants "a separate, independent UI." This needs definition.

**Considerations**:
- The main UI (ThumbnailGrid + SearchBar) serves a "browse and filter" use case
- Semantic search is a "discover similar content" use case — different mental model
- Should the semantic search UI live at a separate route (e.g., `/semantic-search`) or as a mode toggle?

**Validation needed**:
1. Research: How do existing tools (Google Photos, Apple Photos, Immich) handle semantic vs keyword search?
2. Sketch wireframes for 2–3 UI approaches:
   - A) Separate page/screen with text-input + image-drop-zone → grid of results
   - B) Right-click "Find similar" on any image → opens sidebar with results
   - C) Tab/mode toggle in existing UI, reusing ThumbnailGrid
3. Define API contract for the semantic search endpoint (see §5)
4. Determine if both search types should be composable (e.g., keyword filter → then semantic rank)

---

## 3. Architecture Impact (Knowns)

These are well-understood from the existing codebase and don't need investigation:

### Backend Integration Pattern

Following the existing pattern from `routes/search.rs`, a new `routes/semantic.rs` module would:

```
backend/src/
├── lib.rs                              # ← add: pub mod semantic;
├── semantic/
│   ├── mod.rs                          # ← new: SemanticIndexManager (embeddings lifecycle)
│   ├── embedder.rs                     # ← new: trait + OpenRouter impl (with impl swap)
│   └── schema.rs                       # ← new: vector store schema
├── routes/
│   ├── mod.rs                          # ← add: pub mod semantic;
│   └── semantic.rs                     # ← new: POST /semantic/search, POST /semantic/index
└── main.rs                             # ← add: state + route nesting
```

### State pattern:

```rust
pub struct SemanticState {
    pub db: Pool<SqliteConnectionManager>,        // existing media metadata
    pub vector_store: Arc<VectorStore>,            // new: chosen vector DB
    pub embedder: Arc<dyn Embedder + Send + Sync>, // trait object for provider swapping
    pub index_manager: Arc<IndexManager>,          // existing Tantivy (for hybrid)
}
```

### Route nesting (extending main.rs L176–211):

```rust
// Add after search routes:
.nest(
    "/api/v1",
    timeout::apply_timeout(
        routes::semantic::routes().with_state(semantic_state),
        120,  // embedding API calls may be slow
    ),
)
```

### API Contract (proposed):

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/semantic/search` | Natural language or image-based similarity search |
| `POST` | `/semantic/index` | Trigger embedding generation for indexed media |
| `GET`  | `/semantic/status` | Indexing progress, vector count |
| `GET`  | `/semantic/similar/:id` | Find similar images to a given media item |

Request/response schemas need to follow the existing `{data, meta}` envelope pattern.

---

## 4. Cost Estimation Framework

### API Provider Costs (OpenRouter, if Qwen3-VL-Embedding-2B available)

**Assumptions** (based on Qwen3-Embedding-8B pricing of $0.01/M input tokens):
- An image at 1MP resolution tokenizes to approximately 1,000–4,000 vision tokens (ViT patches + LLM projection)
- Conservative estimate: 2,500 tokens per image
- Cost per image: $0.01 × (2,500 / 1,000,000) = **$0.000025/image**
- For 10K images: **$0.25**
- For 100K images: **$2.50**
- For 1M images: **$25.00**

**Self-hosted GPU** (L4 spot instance, ~$0.30/hr):
- At ~4s/image (p50 from Superlinked benchmark): ~900 images/hour
- For 100K images: ~111 GPU-hours → **~$33**
- For 1M images: ~1,111 GPU-hours → **~$333**

**Self-hosted CPU** (amortized, assuming user's machine):
- At 60s/image (conservative): 60 images/hour
- For 100K images: ~1,667 CPU-hours → **$0 (user's hardware)**
- Not practical for initial indexing at this speed — background indexing over days/weeks

**Recommendation**: API for initial indexing, self-hosted GPU for ongoing/re-index operations.

---

## 5. Next Steps / Experiment Plan

### Phase 1 — Validation (1–2 weeks)

| # | Experiment | Owner | Expected Output |
|---|-----------|-------|-----------------|
| E1 | Benchmark Qwen3-VL-Embedding-2B CPU (llama.cpp GGUF Q4_K_M) on real hardware | TBD | p50/p95 time per 1MP image on Ryzen 7/9 |
| E2 | Test OpenRouter availability for Qwen3-VL-Embedding-2B; if unavailable, test Together AI | TBD | Latency, cost, embedding quality for 10 test images |
| E3 | Benchmark vector stores (LanceDB vs sqlite-vec vs Qdrant embedded) | TBD | Query latency, memory, disk for 100K × 2048-dim |
| E4 | Embedding quality at 256/512/768/1024px vs original for 100 ComfyUI images | TBD | recall@10 curve, determine minimum viable resolution |
| E5 | Test vLLM self-hosted Qwen3-VL-Embedding-2B with OpenAI-compatible endpoint | TBD | Confirm embedding consistency with API providers |

### Phase 2 — Design (concurrent with Phase 1)

| # | Deliverable | Owner |
|---|-------------|-------|
| D1 | UI wireframes (2–3 approaches) | TBD |
| D2 | Final API contract (endpoints, request/response schemas) | TBD |
| D3 | Embedder trait design + provider selection | TBD |
| D4 | Vector store selection decision | TBD |

### Phase 3 — Prototype (after Phase 1+2)

| # | Deliverable |
|---|-------------|
| P1 | Working prototype: index 1K images via OpenRouter → vector store → search |
| P2 | End-to-end latency benchmark (image upload → search result) |
| P3 | Cost projection for 100K-image library |

---

## 6. Risks & Unknowns

| Risk | Severity | Mitigation |
|------|----------|------------|
| OpenRouter doesn't support Qwen3-VL-Embedding-2B | 🔴 High | Fallback: Together AI, Fireworks, or direct DashScope |
| CPU embedding too slow for practical use | 🟡 Medium | Accept API-only for v1; self-hosting via GPU for v2 |
| Vector store doesn't scale to 1M vectors | 🟡 Medium | Test with synthetic data before selection; have fallback |
| Embedding quality insufficient for ComfyUI art | 🟢 Low | Qwen3-VL tops MMEB benchmarks; fine-tuning is possible later |
| llama.cpp GGUF support is incomplete | 🟡 Medium | Fall back to vLLM for self-hosting; Transformer-based CPU is backup |

---

## References

- [Qwen3-VL-Embedding Hugging Face](https://huggingface.co/Qwen/Qwen3-VL-Embedding-2B)
- [Qwen3-VL-Embedding Technical Report](https://arxiv.org/abs/2601.04720)
- [Qwen3-VL-Embedding Blog](https://qwen.ai/blog?id=qwen3-vl-embedding)
- [OpenRouter Model Pricing](https://openrouter.ai/models)
- [Superlinked Qwen3-VL-Embedding-2B Benchmarks](https://superlinked.com/models/qwen-qwen3-vl-embedding-2b)
- [llama.cpp Qwen3-VL-Embedding Support Discussion](https://github.com/ggml-org/llama.cpp/discussions/19516)
- [Qwen3-VL Forward Time Issue (GitHub)](https://github.com/QwenLM/Qwen3-VL/issues/1811)
- [Gemini Embedding 2 vs Qwen3 VL Comparison (MindStudio)](https://www.mindstudio.ai/blog/gemini-embedding-2-vs-qwen3-vl-embeddings-comparison)
