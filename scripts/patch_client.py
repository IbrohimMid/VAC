import re

with open('crates/vac_tools/src/mcp/client.rs', 'r') as f:
    content = f.read()

# Add imports
content = content.replace('#[allow(clippy::large_enum_variant)]\nenum McpConnection {',
'''use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

#[allow(clippy::large_enum_variant)]
enum McpConnection {''')

# Add WebSocket variant to McpConnection
content = content.replace('''    Sse {
        base_url: String,
        http: reqwest::Client,
    },
}''', '''    Sse {
        base_url: String,
        http: reqwest::Client,
    },
    WebSocket {
        ws: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    },
}''')

# Add connect logic
content = content.replace('''            McpTransport::Sse { url } => McpConnection::Sse {
                base_url: url.clone(),
                http: self.build_sse_client(url, trust_class)?,
            },
        };''', '''            McpTransport::Sse { url } => McpConnection::Sse {
                base_url: url.clone(),
                http: self.build_sse_client(url, trust_class)?,
            },
            McpTransport::WebSocket { url } => {
                let req = tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(url.clone())
                    .map_err(|e| ToolError::McpError(format!("Invalid WS URL '{}': {e}", url)))?;
                let (ws_stream, _response) = tokio_tungstenite::connect_async(req)
                    .await
                    .map_err(|e| ToolError::McpError(format!("WS connection failed for '{}': {e}", url)))?;
                McpConnection::WebSocket { ws: ws_stream }
            }
        };''')

# Add WebSocket logic to send_request
ws_send_request = '''            McpConnection::WebSocket { ws } => {
                ws.send(Message::Text(request_json.into())).await.map_err(|e| ToolError::McpError(format!("WS send error: {}", e)))?;
                
                while let Some(msg_result) = ws.next().await {
                    let msg = msg_result.map_err(|e| ToolError::McpError(format!("WS receive error: {}", e)))?;
                    if let Message::Text(text) = msg {
                        let response: JsonRpcResponse = serde_json::from_str(&text)
                            .map_err(|e| ToolError::McpError(format!("Parse error: {}", e)))?;
                        
                        if response.id == Some(id) {
                            if let Some(err) = response.error {
                                return Err(ToolError::McpError(format!("MCP error {}: {}", err.code, err.message)));
                            }
                            return response.result.ok_or_else(|| ToolError::McpError("No result in response".into()));
                        }
                    }
                }
                Err(ToolError::McpError("WS stream closed before response".into()))
            }
        }'''

content = content.replace('''                response
                    .result
                    .ok_or_else(|| ToolError::McpError("No result in response".into()))
            }
        }''', '''                response
                    .result
                    .ok_or_else(|| ToolError::McpError("No result in response".into()))
            }
''' + ws_send_request)

# Add WebSocket logic to execute
ws_execute = '''            McpConnection::WebSocket { ws } => {
                ws.send(Message::Text(request_json.into())).await.map_err(|e| ToolError::McpError(format!("WS send error: {}", e)))?;
                
                while let Some(msg_result) = ws.next().await {
                    let msg = msg_result.map_err(|e| ToolError::McpError(format!("WS receive error: {}", e)))?;
                    if let Message::Text(text) = msg {
                        let response: JsonRpcResponse = serde_json::from_str(&text)
                            .map_err(|e| ToolError::McpError(format!("Parse error: {}", e)))?;
                        
                        if response.id == Some(id) {
                            if let Some(err) = response.error {
                                return Err(ToolError::McpError(format!("{}: {}", err.code, err.message)));
                            }
                            return response.result.ok_or_else(|| ToolError::McpError("No result".into()));
                        }
                    }
                }
                Err(ToolError::McpError("WS stream closed before response".into()))
            }
        }'''

content = content.replace('''                response
                    .result
                    .ok_or_else(|| ToolError::McpError("No result".into()))
            }
        }''', '''                response
                    .result
                    .ok_or_else(|| ToolError::McpError("No result".into()))
            }
''' + ws_execute)

# Fix send_notification
ws_notify = '''            McpConnection::WebSocket { ws } => {
                ws.send(Message::Text(request_json.into())).await.map_err(|e| ToolError::McpError(format!("WS send error: {}", e)))?;
                Ok(())
            }
        }'''
content = content.replace('''                    .send()
                    .await
                    .map_err(|e| ToolError::McpError(format!("HTTP: {}", e)))?;
                Ok(())
            }
        }''', '''                    .send()
                    .await
                    .map_err(|e| ToolError::McpError(format!("HTTP: {}", e)))?;
                Ok(())
            }
''' + ws_notify)

# Fix validate_config
content = content.replace('''            (McpTransport::Sse { url }, trust_class) => {
                let parsed = reqwest::Url::parse(url).map_err(|e| {''', '''            (McpTransport::Sse { url }, trust_class) | (McpTransport::WebSocket { url }, trust_class) => {
                let parsed = reqwest::Url::parse(url).map_err(|e| {''')

with open('crates/vac_tools/src/mcp/client.rs', 'w') as f:
    f.write(content)
