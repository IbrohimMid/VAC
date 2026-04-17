# Provider Parity Contract

## Overview
This document outlines the expectations for LLM providers integrated into VAC, ensuring consistency across both streaming and complete request execution. We require that all providers adhere to the `StreamParityContract`.

## Streaming Parity Contract
When an LLM provider is requested to stream its response, it must implement the `StreamParityContract`. This contract guarantees that regardless of the underlying API specifics, the VAC engine receives uniform chunks.

### Contract Requirements
1. **Tool Call Starts**: The stream must emit a `StreamChunk::ToolCallStart` containing the ID and name of the tool before any arguments are streamed.
2. **Tool Call Deltas**: The stream must emit `StreamChunk::ToolCallDelta` chunks containing partial JSON arguments as they arrive.
3. **Tool Call Completion**: If the provider natively emits a completion event, or if VAC needs to synthesize one, the stream must eventually emit a `StreamChunk::ToolCallComplete` for each tool call that was started.
4. **Usage and Finish Reason**: The stream must conclude with a `StreamChunk::Done` containing the total token usage and the finish reason (e.g., `FinishReason::ToolUse` or `FinishReason::Stop`).

## CI Smoke Tests
To ensure ongoing compliance with the `StreamParityContract`, we require CI smoke tests that validate at least 3 providers (e.g., Anthropic, OpenAI, Gemini) against real APIs. These tests execute a basic streaming tool call scenario and verify that all chunks conform to the contract.
