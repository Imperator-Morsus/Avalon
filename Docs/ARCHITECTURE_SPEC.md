# 🏗️ Avalon Project Architecture Specification 🏗️

**Last Updated:** 2026-04-26
**Status:** Actively developing the core orchestration backend.
**Primary Goal:** To create a highly secure, flexible, multi-modal AI coding harness.

---

## 🌐 1. Architectural Pillars

The system is designed around a **Micro-Orchestrator pattern** where core components communicate via standardized, restricted APIs.

**Stack:**
*   **Backend:** Rust (Actix-web)
*   **Frontend:** React (Electron)
*   **Communication:** JSON (primary contract), gRPC/REST (internal service calls).

## 🛡️ 2. Security & Access Control (Mandatory Layer)

The **Security & Permission Manager Service** is the ultimate gatekeeper.

*   **Principle:** Default Deny. Every system action must pass `SecurityManager::check_access`.
*   **Mechanism:** The `SecurityManager` tracks permissions per `calling_module` for specific `path`s, defining allowed `action`s (Read/Write/Delete).
*   **Implementation:** This system is mandatory for running any model component and is now integrated into the `inference_handler`.
*   **Documentation:** Refer to `SECURITY_PROTOCOL.md` for the default policies.

## 📝 3. Core Contracts & Data Flow (Task #3: COMPLETED)

**Endpoint:** `POST /v1/infer`

**A. Input Contract (`InferenceRequest`):**
The contract has been expanded to handle multimodal input:
```json
{
    "prompt": "User's main query text.",
    "user_context": "Optional user-specific context string.",
    "mindmap_payload": { /* Structured data for mindmaps */ },
    "image_archives": [ /* Array of base64 or metadata */ ],
    "other_instances": { /* Generic payload for external sources */ },
    "model_params": { /* Flexible parameters for the model */ }
}
```

**B. Output Contract (`InferenceResponse`):**
```json
{
    "completion": "The generated text completion (expected Markdown format).",
    "model_used": "Model Identifier (e.g., LocalModelV1, CodeReviewModel)",
    "status": "Success/Error"
}
```

## 🧠 4. The Model Orchestrator (Task #1: COMPLETED)

The `ModelInferenceService` now acts as a dedicated **Orchestration Layer**. It is not just a function; it is a multi-stage workflow:

1.  **Security Validation:** Check `SecurityManager` for permission to run.
2.  **Context Gathering:** Calls internal microservices (Image Service, Mindmap Service, etc.) to serialize raw data into text/payloads.
3.  **Prompt Construction:** Builds the final, comprehensive prompt by combining the user prompt and all serialized contexts.
4.  **Inference:** Passes the structured prompt to the target model.
5.  **Output Generation:** Captures the output, expecting **Markdown format**.

## 🖼️ 5. Future/Upcoming Features (Task #2 & #4)

*   **Task #2: Frontend:** Build the React UI to assemble the complex payload and submit it to `/v1/infer`.
*   **Task #4: System/File I/O Manager:** Implement the system that validates and mediates all file system access (e.g., when saving generated documents).

This document serves as our single source of truth for the system's architecture.