import axios from 'axios';
import { InferenceRequest, InferenceResponse } from './contracts/InferenceContract';

const BACKEND_URL = 'http://localhost:8080';

/**
 * Makes a secure POST request to the local backend inference endpoint.
 * @param {InferenceRequest} requestPayload - The full request body containing user query and context.
 * @returns {Promise<InferenceResponse>} The structured response from the backend.
 */
export async function callInferenceAPI(requestPayload) {
    try {
        console.log("Attempting to connect to local backend at:", BACKEND_URL);

        const response = await axios.post(
            `${BACKEND_URL}/v1/infer`,
            requestPayload,
            {
                headers: {
                    'Content-Type': 'application/json'
                }
            }
        );

        // Assuming the backend sends the structured InferenceResponse
        return response.data;

    } catch (error) {
        console.error("Error communicating with the backend:", error);
        throw new Error("Failed to communicate with the local inference service.");
    }
}