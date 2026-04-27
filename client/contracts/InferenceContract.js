// File: D:\Avalon\client\contracts\InferenceContract.js

// Replicating the Rust struct contract for client-side use
export class InferenceRequest {
    constructor(prompt, user_context = null, mindmap_payload = null, image_archives = [], model_params = {}) {
        this.prompt = prompt;
        this.user_context = user_context;
        this.mindmap_payload = mindmap_payload;
        this.image_archives = image_archives;
        this.model_params = model_params;
    }
}

export class InferenceResponse {
    constructor(completion, model_used, status = "Success") {
        this.completion = completion;
        this.model_used = model_used;
        this.status = status;
    }
}