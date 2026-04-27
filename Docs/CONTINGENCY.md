# Avalon Contingency Document

## Project Overview
**Goal:** To build a modern, focused AI coding harness.
**Architecture:** Rust Backend (Actix-web) $\leftrightarrow$ JSON Contract $\leftrightarrow$ React Frontend (Electron).
**Key Components:**
1.  **Backend:** Rust using `actix-web`. Manages the `/v1/infer` endpoint.
2.  **Contract:** `InferenceRequest` / `InferenceResponse` (defined in `src/main.rs`).
3.  **Abstraction:** The `ModelInferenceService` trait is the critical decoupling point.

## Current Project State
**Status:** Task #1 (Implement Model Inference Service) is **COMPLETED**.
**Focus:** The implementation has established the core orchestration layer, integrating security checks and supporting multi-modal context passing.
**Critical Constraints & Scope Creep:**
*   **IMPORTANT:** All development must explicitly disregard the complexity of the "Old Avalon" GUI; we are building a new, clean system.

## Next Steps & Objectives
1.  **Task #2 (Frontend):** Build the React component to consume the stable `/v1/infer` API endpoint.
2.  **Task #4 (System Manager):** Implement the security manager to govern all file system actions.

