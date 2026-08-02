// https://api-docs.deepseek.com/zh-cn/api/create-chat-completion

use serde::{Deserialize, Serialize};
use serde_json::Value;

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

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: Function,
}

#[derive(Debug, Serialize, Deserialize)]
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
    system_fingerprint: String,
    object: String,
    // usage: Usage,
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
            "object": "chat.completion"
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
            "object": "chat.completion"
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
        assert!(json.contains(r#""content":"command output""#), "got: {json}");
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

    // tools 为 None 时请求体不应出现 "tools" 键（skip_serializing_if 生效）
    #[test]
    fn serialize_request_omits_tools_when_none() {
        let json = serde_json::to_string(&sample_request()).unwrap();
        assert!(!json.contains(r#""tools""#), "got: {json}");
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
            "object": "chat.completion"
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
}
