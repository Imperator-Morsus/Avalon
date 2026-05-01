# Avalon v0.2.0-pre-gui Archive Manifest

**Tag:** `v0.2.0-pre-gui`  
**Date:** 2026-04-29  
**Phase:** 1-3 complete, pre-Phase 4 (GitMind-style GUI overhaul)

---

## What's in this archive

### Phase 1 — Unified Knowledge Graph Foundation
- Unified `vault_items` table (replaces vault_documents + vision_images)
- `vault_relationships` graph edges (contains, references, relates_to, teaches, summarizes, contradicts, older_version, newer_version)
- `vault_embeddings` semantic vectors (768-dim f32 via Ollama)
- `vault_notifications` contradiction alerts
- Version chains: `replaces_id`, `version`, `status` (current/archived/conflicted)
- Contradiction detection with confidence threshold (default 0.85)
- Mindmap builds from vault data (`/api/vault/mindmap`)
- FTS5 full-text search across all content types
- Auto-migration from legacy vault_documents + vision_images

### Phase 2 — Embeddings + Auto-ingestion + Transcription
- `EmbeddingService` calls Ollama `nomic-embed-text:v1.5`
- Background embedding queue processor (30s interval, 10 items/batch)
- `POST /api/vault/semantic_search` — cosine similarity over embeddings
- Every chat session auto-saved as `content_type='conversation'`
- `transcribe` tool — ffmpeg + whisper.cpp, stores transcription with `summarizes` relationship
- `which` crate dependency for tool discovery

### Phase 3 — The Librarian Agent + Reasoning
- Built-in agent `"librarian"` (Astra) with vault maintenance tools
- `vault_ingest` — validates `allowed_paths` before ingestion
- `vault_link_items` — creates graph relationships
- `vault_extract_concepts` — calls Ollama to extract key concepts, creates `content_type='concept'` nodes
- `vault_detect_contradiction` — compares versions via LLM, flags contradictions at confidence >= 0.85
- `vault_read_notifications` — returns unread alerts
- Bias-neutral: surfaces conflicts without declaring truth

### Security hardening (applied across all phases)
- Audit logging on all vault endpoints (search, sync, delete, semantic search, get)
- `allowed_paths` validation on `vault_sync` and `transcribe`
- Rate limit on semantic search (limit capped at 50)
- Session permission system maintained
- `SecurityConfig` with block_private_ips, enforce_html_sanitize, require_write/delete_permission

## Files included
- `Avalon_v0.2.0-pre-gui.zip` — full source code archive (191KB)
- `Avalon_v0.2.0-pre-gui.avalon.db` — SQLite vault database backup (104KB)

## To restore
```bash
cd D:/Avalon
git checkout v0.2.0-pre-gui
```

## Next phase (Phase 4 — not included)
GitMind-style interactive graph UI:
- Expandable branches, outline view alongside graph
- Theme customization, node type styling
- Presentation-ready export formats
