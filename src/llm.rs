// https://api-docs.deepseek.com/zh-cn/api/create-chat-completion

use std::time::{Duration, Instant};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub async fn completions(
    client: &Client,
    url: &str,
    token: &str,
    model: &str,
    messages: Vec<Message>,
    tools: Option<Vec<Tool>>,
) -> Result<(Response, Duration), String> {
    let request = Request {
        model: model.to_owned(),
        messages,
        tools,
    };
    println!(
        "request: {}",
        serde_json::to_string_pretty(&request).unwrap()
    );

    let started = Instant::now();
    let response = client
        .post(url)
        .bearer_auth(token)
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let elapsed = started.elapsed();

    if !status.is_success() {
        return Err(format!("HTTP {status}: {body}"));
    }
    let response =
        serde_json::from_str(&body).map_err(|e| format!("json parse failed: {e}\n{body}"))?;

    println!(
        "response: {}",
        serde_json::to_string_pretty(&response).unwrap()
    );

    Ok((response, elapsed))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub messages: Vec<Message>,
    pub model: String,
    // thinking: Option<Thinking>,
    // reasoning_effort: Option<ReasoningEffort>,
    // max_tokens: Option<i64>,
    // response_format: Option<ResponseFormat>
    // stop,
    // stream,
    // stream_options,
    // temperature,
    // top_p
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    // tool_choice
    // logprobs
    // top_logprobs,
    // user_id
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum Message {
    System {
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    User {
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    Assistant {
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        // prefix: Bool,
        // reasoning_content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<ToolCall>>,
    },
    Tool {
        content: String,
        tool_call_id: String,
    },
}

// enum ReasoningEffort {
//     low,
//     high,
//     max,
// }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: Function,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    pub description: String,
    pub name: String,
    pub parameters: Value,
    // strict: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    id: String,
    pub choices: Vec<Choice>,
    created: i64,
    model: String,
    system_fingerprint: Option<String>,
    object: String,
    pub usage: Usage,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Choice {
    pub finish_reason: FinishReason,
    index: i64,
    pub message: ResponseMessage,
    // logprobs: Option<Logprobs>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
    ToolCalls,
    InsufficientSystemResource,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseMessage {
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    role: Role,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    tool_call_type: ToolCallType,
    pub function: ResponseFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ToolCallType {
    Function,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Role {
    Assistant,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: isize,
    pub completion_tokens: isize,
    pub total_tokens: isize,

    // deepseek only
    pub prompt_cache_hit_tokens: Option<isize>,
    // deepseek only
    pub prompt_cache_miss_tokens: Option<isize>,

    pub prompt_tokens_details: Option<PromptTokensDetails>,
    pub completion_tokens_details: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PromptTokensDetails {
    pub cached_tokens: isize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> Request {
        Request {
            model: "deepseek-chat".to_string(),
            messages: vec![
                Message::System {
                    content: "You are a helpful assistant.".to_string(),
                    name: None,
                },
                Message::User {
                    content: "Hello!".to_string(),
                    name: None,
                },
            ],
            tools: None,
        }
    }

    // role tag + snake_case 改名是否生效；model 是否带上
    #[test]
    fn serialize_message_uses_role_tag_and_snake_case() {
        let json = serde_json::to_string(&sample_request()).unwrap();
        assert!(json.contains(r#""role":"system""#), "got: {json}");
        assert!(json.contains(r#""role":"user""#), "got: {json}");
        assert!(json.contains(r#""model":"deepseek-chat""#), "got: {json}");
    }

    // name 为 None 时整键不应出现（skip_serializing_if 生效）
    #[test]
    fn serialize_omits_name_when_none() {
        let json = serde_json::to_string(&sample_request()).unwrap();
        assert!(!json.contains(r#""name""#), "got: {json}");
    }

    // name 为 Some 时应输出
    #[test]
    fn serialize_keeps_name_when_some() {
        let req = Request {
            model: "deepseek-chat".to_string(),
            messages: vec![Message::User {
                content: "Hi".to_string(),
                name: Some("alice".to_string()),
            }],
            tools: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""name":"alice""#), "got: {json}");
    }

    // 序列化后能否无损反序列化回相同结构（往返）
    #[test]
    fn serialize_then_parse_roundtrips_role() {
        let json = serde_json::to_string(&sample_request()).unwrap();
        let parsed: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.model, "deepseek-chat");
        assert_eq!(parsed.messages.len(), 2);
        assert!(matches!(parsed.messages[0], Message::System { .. }));
        assert!(matches!(parsed.messages[1], Message::User { .. }));
    }

    // 解析典型的 DeepSeek 响应，覆盖 Response / Choice / ResponseMessage / Role
    #[test]
    fn deserialize_full_response() {
        let raw = r#"{
            "id": "chatcmpl-1",
            "choices": [
                {
                    "finish_reason": "stop",
                    "index": 0,
                    "message": {
                        "content": "hello",
                        "role": "assistant"
                    }
                }
            ],
            "created": 1700000000,
            "model": "deepseek-chat",
            "system_fingerprint": "fp_1",
            "object": "chat.completion",
            "usage": {
                "completion_tokens": 12,
                "prompt_tokens": 34,
                "prompt_cache_hit_tokens": 10,
                "prompt_cache_miss_tokens": 24,
                "total_tokens": 46,
                "completion_tokens_details": {"reasoning_tokens": 8}
            }
        }"#;
        let resp: Response = serde_json::from_str(raw).unwrap();
        assert_eq!(resp.id, "chatcmpl-1");
        assert_eq!(resp.model, "deepseek-chat");
        assert_eq!(resp.object, "chat.completion");
        assert_eq!(resp.created, 1700000000);
        assert_eq!(resp.choices.len(), 1);
        let choice = &resp.choices[0];
        assert!(matches!(choice.finish_reason, FinishReason::Stop));
        assert_eq!(choice.index, 0);
        assert_eq!(choice.message.content.as_deref(), Some("hello"));
        assert!(matches!(choice.message.role, Role::Assistant));
    }

    // content/reasoning_content/tool_calls 缺失也能解析（API 可能不返回）
    #[test]
    fn deserialize_message_with_optional_fields_absent() {
        let raw = r#"{
            "id": "x",
            "choices": [
                {
                    "finish_reason": "length",
                    "index": 1,
                    "message": { "role": "assistant" }
                }
            ],
            "created": 1,
            "model": "m",
            "system_fingerprint": "",
            "object": "chat.completion",
            "usage": {
                "completion_tokens": 12,
                "prompt_tokens": 34,
                "prompt_cache_hit_tokens": 10,
                "prompt_cache_miss_tokens": 24,
                "total_tokens": 46,
                "completion_tokens_details": {"reasoning_tokens": 8}
            }
        }"#;
        let resp: Response = serde_json::from_str(raw).unwrap();
        let msg = &resp.choices[0].message;
        assert!(msg.content.is_none());
        assert!(msg.reasoning_content.is_none());
        assert!(msg.tool_calls.is_none());
        assert!(matches!(
            resp.choices[0].finish_reason,
            FinishReason::Length
        ));
    }

    // ===== tool-calling 链路相关 =====

    // assistant 消息带 tool_calls 时的序列化：多轮回写历史的核心
    // 下一轮请求里这条消息必须带 tool_calls，否则 tool 结果找不到对应调用 → 400
    #[test]
    fn serialize_assistant_with_tool_calls() {
        let req = Request {
            model: "deepseek-chat".to_string(),
            messages: vec![Message::Assistant {
                content: None,
                name: None,
                tool_calls: Some(vec![ToolCall {
                    id: "call_abc".to_string(),
                    tool_call_type: ToolCallType::Function,
                    function: ResponseFunction {
                        name: "bash".to_string(),
                        arguments: r#"{"command":"ls"}"#.to_string(),
                    },
                }]),
            }],
            tools: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""role":"assistant""#), "got: {json}");
        assert!(json.contains(r#""tool_calls""#), "got: {json}");
        assert!(json.contains(r#""id":"call_abc""#), "got: {json}");
        assert!(json.contains(r#""type":"function""#), "got: {json}");
        assert!(json.contains(r#""name":"bash""#), "got: {json}");
        assert!(json.contains("command"), "got: {json}");
    }

    // assistant 同时带 content 和 tool_calls 的序列化：真实 API 常返回既有文本又有工具调用
    // 的消息，需要确保两个字段都能一起正确输出
    #[test]
    fn serialize_assistant_with_content_and_tool_calls() {
        let req = Request {
            model: "deepseek-chat".to_string(),
            messages: vec![Message::Assistant {
                content: Some("let me run that for you".to_string()),
                name: None,
                tool_calls: Some(vec![ToolCall {
                    id: "call_abc".to_string(),
                    tool_call_type: ToolCallType::Function,
                    function: ResponseFunction {
                        name: "bash".to_string(),
                        arguments: r#"{"command":"ls"}"#.to_string(),
                    },
                }]),
            }],
            tools: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""role":"assistant""#), "got: {json}");
        assert!(json.contains("let me run that for you"), "got: {json}");
        assert!(json.contains(r#""tool_calls""#), "got: {json}");
        assert!(json.contains(r#""id":"call_abc""#), "got: {json}");
    }

    // assistant 的 tool_calls 为 None 时整键不应出现（skip_serializing_if 生效）
    #[test]
    fn serialize_assistant_omits_tool_calls_when_none() {
        let req = Request {
            model: "deepseek-chat".to_string(),
            messages: vec![Message::Assistant {
                content: Some("hi".to_string()),
                name: None,
                tool_calls: None,
            }],
            tools: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains(r#""tool_calls""#), "got: {json}");
    }

    // tool 结果消息序列化：role=tool，且带上 tool_call_id
    #[test]
    fn serialize_tool_message_carries_tool_call_id() {
        let req = Request {
            model: "deepseek-chat".to_string(),
            messages: vec![Message::Tool {
                content: "command output".to_string(),
                tool_call_id: "call_abc".to_string(),
            }],
            tools: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""role":"tool""#), "got: {json}");
        assert!(json.contains(r#""tool_call_id":"call_abc""#), "got: {json}");
        assert!(
            json.contains(r#""content":"command output""#),
            "got: {json}"
        );
    }

    // Tool 定义序列化时字段名应是 "type" 而非 "tool_type"
    // 验证 #[serde(rename = "type")] 生效，否则 API 不认这个工具
    #[test]
    fn serialize_tool_definition_renames_type_field() {
        let req = Request {
            model: "deepseek-chat".to_string(),
            messages: vec![],
            tools: Some(vec![Tool {
                tool_type: "function".to_string(),
                function: Function {
                    name: "bash".to_string(),
                    description: "run".to_string(),
                    parameters: serde_json::json!({"type": "object"}),
                },
            }]),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""type":"function""#), "got: {json}");
        assert!(!json.contains(r#""tool_type""#), "got: {json}");
    }

    // Tool 定义往返无损：parameters 是非平凡 JSON 对象时也要保证序列化/反序列化不丢字段
    // （Tool 没 derive PartialEq，所以逐字段断言而非整体 assert_eq）
    #[test]
    fn serialize_deserialize_tool_definition_roundtrips() {
        let params = serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"}
            },
            "required": ["path"]
        });
        let tool = Tool {
            tool_type: "function".to_string(),
            function: Function {
                description: "run a shell command".to_string(),
                name: "bash".to_string(),
                parameters: params.clone(),
            },
        };
        let json = serde_json::to_string(&tool).unwrap();
        let parsed: Tool = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tool_type, tool.tool_type);
        assert_eq!(parsed.function.name, tool.function.name);
        assert_eq!(parsed.function.description, tool.function.description);
        assert_eq!(parsed.function.parameters, params);
    }

    // tools 为 None 时请求体不应出现 "tools" 键（skip_serializing_if 生效）
    #[test]
    fn serialize_request_omits_tools_when_none() {
        let json = serde_json::to_string(&sample_request()).unwrap();
        assert!(!json.contains(r#""tools""#), "got: {json}");
    }

    // tools 为 Some 时请求体应出现 "tools" 键：与上一条成对，坐实 skip_serializing_if 仅在
    // None 时跳过，Some 时正常输出
    #[test]
    fn serialize_request_includes_tools_when_some() {
        let req = Request {
            model: "deepseek-chat".to_string(),
            messages: vec![],
            tools: Some(vec![Tool {
                tool_type: "function".to_string(),
                function: Function {
                    description: "run".to_string(),
                    name: "bash".to_string(),
                    parameters: serde_json::json!({"type": "object"}),
                },
            }]),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""tools""#), "got: {json}");
    }

    // 解析带 tool_calls 的响应：finish_reason=tool_calls 是 agent_loop 决定走工具分支的信号
    #[test]
    fn deserialize_response_with_tool_calls() {
        let raw = r#"{
            "id": "chatcmpl-2",
            "choices": [{
                "finish_reason": "tool_calls",
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_xyz",
                        "type": "function",
                        "function": {
                            "name": "bash",
                            "arguments": "{\"command\":\"pwd\"}"
                        }
                    }]
                }
            }],
            "created": 2,
            "model": "deepseek-chat",
            "system_fingerprint": "",
            "object": "chat.completion",
            "usage": {
                "completion_tokens": 12,
                "prompt_tokens": 34,
                "prompt_cache_hit_tokens": 10,
                "prompt_cache_miss_tokens": 24,
                "total_tokens": 46,
                "completion_tokens_details": {"reasoning_tokens": 8}
            }
        }"#;
        let resp: Response = serde_json::from_str(raw).unwrap();
        let choice = &resp.choices[0];
        assert!(matches!(choice.finish_reason, FinishReason::ToolCalls));
        assert!(choice.message.content.is_none());
        let tool_calls = choice
            .message
            .tool_calls
            .as_ref()
            .expect("tool_calls missing");
        assert_eq!(tool_calls.len(), 1);
        let tc = &tool_calls[0];
        assert_eq!(tc.id, "call_xyz");
        assert!(matches!(tc.tool_call_type, ToolCallType::Function));
        assert_eq!(tc.function.name, "bash");
        assert_eq!(tc.function.arguments, r#"{"command":"pwd"}"#);
    }

    // ===== 反序列化补全覆盖 =====

    // usage 各字段应被正确解析：token 计数和 cache 命中/未命中是成本核算的依据
    #[test]
    fn deserialize_usage_fields_parsed() {
        let raw = r#"{
            "id": "u1",
            "choices": [{
                "finish_reason": "stop",
                "index": 0,
                "message": {"content": "hi", "role": "assistant"}
            }],
            "created": 1,
            "model": "m",
            "system_fingerprint": "",
            "object": "chat.completion",
            "usage": {
                "completion_tokens": 12,
                "prompt_tokens": 34,
                "prompt_cache_hit_tokens": 10,
                "prompt_cache_miss_tokens": 24,
                "total_tokens": 46,
                "completion_tokens_details": {"reasoning_tokens": 8}
            }
        }"#;
        let resp: Response = serde_json::from_str(raw).unwrap();
        let usage = &resp.usage;
        assert_eq!(usage.completion_tokens, 12);
        assert_eq!(usage.prompt_tokens, 34);
        assert_eq!(usage.prompt_cache_hit_tokens, Some(10));
        assert_eq!(usage.prompt_cache_miss_tokens, Some(24));
        assert_eq!(usage.total_tokens, 46);
        assert_eq!(
            usage.completion_tokens_details["reasoning_tokens"],
            serde_json::json!(8)
        );
    }

    // FinishReason 全部 5 个变体的 rename 都应生效，特别是 content_filter 和
    // insufficient_system_resource 这种多词组合，容易在 snake_case 改名时出错
    #[test]
    fn finish_reason_all_variants_roundtrip() {
        let cases = [
            (FinishReason::Stop, "stop"),
            (FinishReason::Length, "length"),
            (FinishReason::ContentFilter, "content_filter"),
            (FinishReason::ToolCalls, "tool_calls"),
            (
                FinishReason::InsufficientSystemResource,
                "insufficient_system_resource",
            ),
        ];
        for (variant, json_str) in cases {
            let raw = format!(
                r#"{{
                    "id": "x",
                    "choices": [{{
                        "finish_reason": "{json_str}",
                        "index": 0,
                        "message": {{"role": "assistant"}}
                    }}],
                    "created": 1,
                    "model": "m",
                    "system_fingerprint": "",
                    "object": "chat.completion",
                    "usage": {{
                        "completion_tokens": 0,
                        "prompt_tokens": 0,
                        "prompt_cache_hit_tokens": 0,
                        "prompt_cache_miss_tokens": 0,
                        "total_tokens": 0,
                        "completion_tokens_details": {{}}
                    }}
                }}"#
            );
            let resp: Response = serde_json::from_str(&raw)
                .unwrap_or_else(|e| panic!("parse failed for {json_str}: {e}"));
            assert_eq!(
                resp.choices[0].finish_reason, variant,
                "mismatch for finish_reason={json_str}"
            );
        }
    }

    // DeepSeek-reasoner 风格响应：message 里的 reasoning_content 应被解析为 Some
    // （这是 reasoning model 的思维链字段，agent 可能用来展示思考过程）
    #[test]
    fn deserialize_message_with_reasoning_content() {
        let raw = r#"{
            "id": "r1",
            "choices": [{
                "finish_reason": "stop",
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "answer",
                    "reasoning_content": "let me think..."
                }
            }],
            "created": 1,
            "model": "deepseek-reasoner",
            "system_fingerprint": "",
            "object": "chat.completion",
            "usage": {
                "completion_tokens": 12,
                "prompt_tokens": 34,
                "prompt_cache_hit_tokens": 10,
                "prompt_cache_miss_tokens": 24,
                "total_tokens": 46,
                "completion_tokens_details": {"reasoning_tokens": 8}
            }
        }"#;
        let resp: Response = serde_json::from_str(raw).unwrap();
        let msg = &resp.choices[0].message;
        assert_eq!(msg.reasoning_content.as_deref(), Some("let me think..."));
        assert_eq!(msg.content.as_deref(), Some("answer"));
    }

    // 并行工具调用：一条 assistant 消息带多个 tool_calls 是真实 API 行为（一次返回多个工具请求）
    // 必须保证两个 call 各自的 id/name/arguments 都正确解析，不能只取第一个
    #[test]
    fn deserialize_response_with_multiple_tool_calls() {
        let raw = r#"{
            "id": "multi",
            "choices": [{
                "finish_reason": "tool_calls",
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [
                        {
                            "id": "call_a",
                            "type": "function",
                            "function": {"name": "bash", "arguments": "{\"command\":\"ls\"}"}
                        },
                        {
                            "id": "call_b",
                            "type": "function",
                            "function": {"name": "read", "arguments": "{\"path\":\"/tmp/x\"}"}
                        }
                    ]
                }
            }],
            "created": 9,
            "model": "deepseek-chat",
            "system_fingerprint": "",
            "object": "chat.completion",
            "usage": {
                "completion_tokens": 12,
                "prompt_tokens": 34,
                "prompt_cache_hit_tokens": 10,
                "prompt_cache_miss_tokens": 24,
                "total_tokens": 46,
                "completion_tokens_details": {"reasoning_tokens": 8}
            }
        }"#;
        let resp: Response = serde_json::from_str(raw).unwrap();
        let tool_calls = resp.choices[0]
            .message
            .tool_calls
            .as_ref()
            .expect("tool_calls missing");
        assert_eq!(tool_calls.len(), 2);

        assert_eq!(tool_calls[0].id, "call_a");
        assert_eq!(tool_calls[0].function.name, "bash");
        assert_eq!(tool_calls[0].function.arguments, r#"{"command":"ls"}"#);

        assert_eq!(tool_calls[1].id, "call_b");
        assert_eq!(tool_calls[1].function.name, "read");
        assert_eq!(tool_calls[1].function.arguments, r#"{"path":"/tmp/x"}"#);
    }

    // 负面测试：Response 缺 usage 字段应反序列化失败。呼应上面的修复——usage 是必需字段，
    // DeepSeek 总会返回，缺了就是协议异常，不应静默成功
    #[test]
    fn deserialize_response_without_usage_fails() {
        let raw = r#"{
            "id": "no-usage",
            "choices": [{
                "finish_reason": "stop",
                "index": 0,
                "message": {"role": "assistant"}
            }],
            "created": 1,
            "model": "m",
            "system_fingerprint": "",
            "object": "chat.completion"
        }"#;
        let result: Result<Response, _> = serde_json::from_str(raw);
        assert!(result.is_err(), "expected error when usage is missing");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("usage"),
            "error should mention usage, got: {err}"
        );
    }
}
