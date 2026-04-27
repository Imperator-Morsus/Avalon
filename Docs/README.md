# 🏰 Project Avalon: Context-Aware AI Harness

## 🚀 Quick Start Guide

  This repository contains a fully modularized, secure AI coding harness designed to accept, process, and act upon
  complex contextual data (code, images, documents) via a secure local API.

### 📦 Prerequisites

* **Runtime:** Node.js (for frontend)

* **Backend:** Rust toolchain (`cargo`, `actix-web`)

* **Database:** PostgreSQL (Requires local setup or Docker Compose for connectivity)

* **Containerization:** Docker & Docker Compose (Recommended for deployment)
  
  ### ⚙️ Setup & Installation
1. **Clone the Repository:**
   
   ```bash
   git clone <YOUR_REPO_URL>
   cd <YOUR_REPO_NAME>
   ```

2. **Environment Variables:**
   Create a file named `.env` in the project root and populate it with required keys (e.g., `DATABASE_URL`,
   `API_KEY`).
   
   ```bash
   # Example of required variables:
   DATABASE_URL="postgres://user:pass@localhost:5432/dbname"
   SECRET_KEY="a_strong_secret_key"
   ```

3. **Run Dependencies:**
   
   ```bash
   # Build backend services
   cargo build --release
   ```
   
   ### Running the Application
   
   You can run the backend service by executing the compiled binary:
   
   ```bash
   cargo run --bin your_backend_binary_name
   
   The service will typically start on http://localhost:8080.
   
   ```

  ---

  Core Components Overview

1. The API Server (Backend)
- Function: Handles all incoming requests, manages state, and orchestrates the call flow.
- Key Endpoint: All external communication routes through the defined endpoint structure.
- Security: All requests must pass through authentication middleware.
2. The Frontend Client
- Function: Provides the user interface for interacting with the API.
- Implementation: Built using modern web frameworks.
- Usage: Loaded via the primary entry point.
3. Data Modeling
- Core Concept: All data structures are defined in a shared model layer, ensuring consistency between the client and
  server.
- Key Principle: Strict type enforcement prevents runtime errors.

  ---

  ⚠️ Security Warning

  All endpoints are protected by role-based access control (RBAC). Unauthorized access attempts will result in a 401
  Unauthorized response.
  ```
